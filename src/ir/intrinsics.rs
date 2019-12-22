/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 12/22/19 3:59 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

use crate::ir::{IRGenerator, PtrEqRc};
use crate::mir::nodes::Function;
use crate::mir::{MModule, MutRc};
use inkwell::types::BasicType;
use inkwell::types::BasicTypeEnum;
use inkwell::values::FunctionValue;
use inkwell::AddressSpace;
use std::cell::RefMut;

pub(super) fn fill_intrinsic_functions(gen: &mut IRGenerator, module: &MutRc<MModule>) {
    for global in module.borrow().globals.values() {
        let ir = gen.functions[&PtrEqRc::new(global)];
        let func = global.type_.as_function();
        fill_function(gen, func.borrow_mut(), ir)
    }
}

fn fill_function(gen: &mut IRGenerator, func: RefMut<Function>, ir: FunctionValue) {
    // Trim the module name and generic parameters off the function
    // "std/mod:func_name<A, B>" -> "func_name"
    let name = &func
        .name
        .rsplit(':')
        .next()
        .unwrap()
        .split('<')
        .next()
        .unwrap();
    if name.len() < 8 || &name[..8] != "gelixrs_" {
        return;
    }

    gen.blocks.clear();
    let bb = gen.context.append_basic_block(&ir, "entry");
    gen.builder.position_at_end(&bb);
    gen.build_parameter_alloca(&func, ir);

    // The 8 strips the "gelixrs_" all IR functions start with
    match &name[8..] {
        // Returns the size of the type. Implemented by
        // indexing a null pointer (0x0) at element 1
        // and returning the offset from 0.
        // Credit: http://nondot.org/sabre/LLVMNotes/SizeOf-OffsetOf-VariableSizedStructs.txt
        "get_type_size" => {
            let i = gen.context.i64_type();
            let mir_ty = func.context.type_aliases.values().next().unwrap();
            let typ = gen
                .to_ir_type_no_ptr(&mir_ty)
                .ptr_type(AddressSpace::Generic);
            let ptr_size = unsafe {
                gen.builder
                    .build_gep(typ.const_null(), &[i.const_int(1, false)], "size")
            };
            gen.builder
                .build_return(Some(&gen.builder.build_ptr_to_int(ptr_size, i, "sizeI")));
        }

        "deref_ptr" => {
            let ret_ty = ir.get_type().get_return_type().unwrap();
            let int = *ir.get_first_param().unwrap().as_int_value();

            if let BasicTypeEnum::PointerType(ret_ty) = ret_ty {
                let ptr = gen.builder.build_int_to_ptr(int, ret_ty, "ptr");
                gen.builder.build_return(Some(&ptr));
            } else {
                let ptr = gen.builder.build_int_to_ptr(
                    int,
                    ret_ty.ptr_type(AddressSpace::Generic),
                    "ptr",
                );
                gen.builder
                    .build_return(Some(&gen.builder.build_load(ptr, "load-ptr")));
            }
        }

        "set_ptr" => {
            let dat = ir.get_last_param().unwrap();
            let ty = dat.get_type();
            let int = *ir.get_first_param().unwrap().as_int_value();

            if let BasicTypeEnum::PointerType(ty) = ty {
                let ptr = gen.builder.build_int_to_ptr(int, ty, "ptr");
                let dat = gen.builder.build_load(*dat.as_pointer_value(), "loaddat");
                gen.builder.build_store(ptr, dat);
            } else {
                let ptr =
                    gen.builder
                        .build_int_to_ptr(int, ty.ptr_type(AddressSpace::Generic), "ptr");
                gen.builder.build_store(ptr, dat);
            }
            gen.builder.build_return(None);
        }

        _ => (),
    }
}
