#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Expression {
    {% for item in items %}{{ item.name }}({{ item.type }}),{% endfor %}
}

impl Expression {
    #[allow(unused)]
    pub fn cast(node: CSTNode) -> Option<Self> {
        {% for item in items %}
        if node.kind() == SyntaxKind::{{ item.kind }} {
            return Some(Self::{{ item.name }}({{ item.type }}::cast(node).unwrap()))
        }{% endfor %}
        None
    }

    pub fn cst(&self) -> CSTNode {
        match self {
            {% for item in items %}Self::{{ item.name }}(inner) => inner.cst(),{% endfor %}
        }
    }
}