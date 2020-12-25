use std::{cell::RefCell, collections::HashMap, rc::Rc};

use crate::{eat, eatc, result::EmitGIRError, GIRGenerator};
use ast::CSTNode;
use common::{mutrc_new, MutRc};
use error::{GErr, Res};
use gir_nodes::{
    declaration::{ADTType, IRAdt, IRFunction, LocalVariable},
    types::{TypeParameter, TypeParameterBound, TypeParameters},
    Declaration, Function, IFaceImpl, Type, ADT,
};
use indexmap::IndexMap;
use smol_str::SmolStr;
use syntax::kind::SyntaxKind;

impl GIRGenerator {
    pub(super) fn declare_adts(&mut self, ast: &ast::Module) {
        for ast in ast.adts() {
            let name = ast.name();
            self.try_reserve_name(&name.cst, &name.name());
            self.adt_from_ast(ast, None);
        }
    }

    fn adt_from_ast(&mut self, ast: ast::Adt, parent: Option<MutRc<ADT>>) -> MutRc<ADT> {
        let ty = match ast.kind() {
            SyntaxKind::Class => ADTType::Class {
                external: ast.modifiers().any(|m| m == SyntaxKind::Extern),
            },

            SyntaxKind::Interface => ADTType::Interface,

            SyntaxKind::Enum => ADTType::Enum {
                cases: HashMap::new(),
            },

            // Enum cases can have multiple starting tokens, just use the catch-all
            _ => ADTType::EnumCase {
                parent: parent.clone().expect("Unknown ADT?"),
                simple: false,
            },
        };

        let name = ast.name();
        let type_parameters = self.ast_generics_to_gir(
            name.type_parameters(),
            parent.map(|p| Rc::clone(&p.borrow().type_parameters)),
        );
        let adt = mutrc_new(ADT {
            name: name.name(), // TODO: FIXME: Enum case names need changing
            fields: IndexMap::with_capacity(10),
            methods: IndexMap::with_capacity(10),
            constructors: Vec::with_capacity(5),
            ir: IRAdt::new(!type_parameters.is_empty()),
            type_parameters,
            ty,
            ast,
            module: Rc::clone(&self.module),
        });

        self.module
            .borrow_mut()
            .declarations
            .insert(name.name(), Declaration::Adt(Rc::clone(&adt)));

        self.maybe_enum_cases(&adt);
        adt
    }

    fn maybe_enum_cases(&mut self, adt_rc: &MutRc<ADT>) {
        if matches!(adt_rc.borrow().ty, ADTType::Enum { .. }) {
            let enum_cases = {
                let adt = adt_rc.borrow();
                adt.ast
                    .cases()
                    .map(|case| {
                        (
                            case.name().name(),
                            self.adt_from_ast(case, Some(Rc::clone(&adt_rc))),
                        )
                    })
                    .collect()
            };
            if let ADTType::Enum { ref mut cases } = &mut adt_rc.borrow_mut().ty {
                *cases = enum_cases;
            }
        }
    }

    /// Takes a list of type parameters of an AST node and
    /// returns it's GIR representation. Can log an error
    /// if type bound cannot be resolved.
    fn ast_generics_to_gir<T: Iterator<Item = ast::TypeParameter>>(
        &mut self,
        params: T,
        parent_params: Option<Rc<TypeParameters>>,
    ) -> Rc<TypeParameters> {
        let parent_size = parent_params.as_ref().map(|g| g.len()).unwrap_or(0);
        let param_iter = params.enumerate().map(|param| {
            TypeParameter {
                name: param.1.name(),
                index: param.0 + parent_size,
                bound: self
                    .bound_from_ast(param.1.bound().as_ref())
                    .unwrap_or_else(|e| {
                        self.error(e);
                        TypeParameterBound::default() // doesn't matter anymore, compilation failed anyway
                    }),
            }
        });

        let params = Rc::new(match parent_params {
            Some(parent) => parent.iter().cloned().chain(param_iter).collect(),
            None => param_iter.collect(),
        });
        self.set_context(&params);
        params
    }

    pub(super) fn declare_iface_impls(&mut self, ast: &ast::Module) {
        for im in ast.impls() {
            self.declare_impl(im)
        }
    }

    fn declare_impl(&mut self, iface_impl: ast::IfaceImpl) {
        let implementor = eat!(self, self.find_type(&iface_impl.implementor()));

        let iface = eat!(self, self.find_type(&iface_impl.iface()));
        if !iface.is_strong_ref() || !iface.as_strong_ref().ty.borrow().ty.is_interface() {
            self.err(iface_impl.iface().cst(), GErr::E307);
            return;
        }
        let iface_adt = iface.as_strong_ref();

        let impls = self.get_iface_impls(&implementor);
        let gir_impl = IFaceImpl {
            implementor,
            iface: iface.as_strong_ref().clone(),
            methods: HashMap::with_capacity(iface_adt.ty.borrow().methods.len()),
            module: Rc::clone(&self.module),
            ast: iface_impl.clone(),
        };
        let already_defined = impls
            .borrow_mut()
            .interfaces
            .insert(iface, gir_impl)
            .is_some();
        if already_defined {
            self.err(iface_impl.iface().cst, GErr::E306);
        }
    }

    pub(super) fn declare_functions(&mut self, ast: &ast::Module) {
        for ast in ast.functions() {
            eatc!(self, self.declare_function(ast));
        }
    }

    pub(crate) fn declare_function(&mut self, func: ast::Function) -> Res<MutRc<Function>> {
        let name = func.sig().name();
        self.try_reserve_name(&name.cst, &name.name());

        let function = self.function_from_ast(func, None, None)?;
        self.module
            .borrow_mut()
            .declarations
            .insert(name.name(), Declaration::Function(Rc::clone(&function)));
        self.maybe_set_main_fn(&function, &name.cst);
        Ok(function)
    }

    /// Creates a function from AST. See create_function for post-AST verification.
    pub(crate) fn function_from_ast(
        &mut self,
        func: ast::Function,
        this_param: Option<(SmolStr, Type)>,
        parent_type_params: Option<Rc<TypeParameters>>,
    ) -> Res<MutRc<Function>> {
        let signature = func.sig();
        let name = signature.name();
        let type_parameters = self.ast_generics_to_gir(name.type_parameters(), parent_type_params);
        let ret_type = signature
            .ret_type()
            .map(|ty| self.find_type(&ty))
            .transpose()?;

        self.create_function(FnSig {
            name: name.name(),
            params: box this_param.into_iter().map(Ok).chain(
                signature
                    .parameters()
                    .map(|ast| Ok((ast.name(), self.find_type(&ast._type())?))),
            ),
            type_parameters,
            ret_type,
            ast: Some(func),
        })
    }

    /// Creates a function. It is not put into the module and its name is
    /// not reserved, making this function suitable for methods or
    /// functions not written by the user.
    /// The main use of this method is validation of the function.
    ///
    /// `this_arg` indicates that the function is a method
    /// with some kind of receiver, with the 'this' parameter
    /// added to the 0th position of the parameters and the
    /// function renamed to '$receiver-$name'.
    pub(crate) fn create_function(&self, sig: FnSig) -> Res<MutRc<Function>> {
        let ret_type = sig.ret_type.unwrap_or_default();
        if !ret_type.can_escape() {
            self.err(
                sig.ast.as_ref().unwrap().sig().ret_type().unwrap().cst,
                GErr::E308,
            );
        }

        let parameters = sig
            .params
            .map(|param| {
                let (name, ty) = param?;
                Ok(Rc::new(LocalVariable {
                    name,
                    ty,
                    mutable: false,
                }))
            })
            .collect::<Res<_>>()?;

        let function = mutrc_new(Function {
            name: sig.name,
            parameters,
            variadic: sig
                .ast
                .as_ref()
                .map(|a| a.modifiers().any(|m| m == SyntaxKind::Variadic))
                .unwrap_or(false),
            exprs: Vec::with_capacity(4),
            variables: Default::default(),
            ret_type,
            ast: sig.ast,
            module: Rc::clone(&self.module),
            ir: RefCell::new(IRFunction::new(!sig.type_parameters.is_empty())),
            type_parameters: sig.type_parameters,
        });
        self.module
            .borrow_mut()
            .functions
            .push(Rc::clone(&function));
        Ok(function)
    }

    fn maybe_set_main_fn(&mut self, func: &MutRc<Function>, err_cst: &CSTNode) {
        if func.borrow().name == "main" {
            let res = self
                .intrinsics
                .set_main_fn(func)
                .or_err(err_cst, GErr::E305);
            self.eat(res);
        }
    }
}

pub(crate) struct FnSig<'a> {
    pub name: SmolStr,
    pub params: Box<dyn Iterator<Item = Res<(SmolStr, Type)>> + 'a>,
    pub type_parameters: Rc<TypeParameters>,
    pub ret_type: Option<Type>,
    pub ast: Option<ast::Function>,
}
