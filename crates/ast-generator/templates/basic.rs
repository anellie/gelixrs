#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct {{ name }} {
    cst: CSTNode,
}
impl {{ name }} {
    #[allow(unused)]
    pub fn cast(node: CSTNode) -> Option<Self> {
        if let SyntaxKind::{{ kind }} = node.kind() {
            Some(Self { cst: node })
        } else {
            None
        }
    }
    {% for item in items %}
    pub fn {{ item.name }}(&self) -> {{ item.type }} {
        self.cst.{{ item.strategy }}
    }{% endfor %}
}