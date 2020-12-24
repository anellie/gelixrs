#[derive(Debug)]
pub struct {{ name }} {
    pub cst: CSTNode,
    pub path: ModulePath,
    pub src: Rc<String>,
}
impl {{ name }} {
    pub fn new(path: &ModPath, src: &Rc<String>, cst: ParseResult) -> Self {
        Self {
            path: Rc::new(path.clone()),
            src: Rc::clone(src),
            cst: cst.root()
        }
    }

    {% for item in items %}
    pub fn {{ item.name }}(&self) -> {{ item.type }} {
        self.cst.{{ item.strategy }}
    }{% endfor %}
}