use rowan::SmolStr;
use syntax::kind::SyntaxKind;

#[derive(Debug, Clone)]
pub(crate) enum Event {
    StartNode(SyntaxKind),
    StartNodeAt { kind: SyntaxKind, checkpoint: usize },
    AddToken { kind: SyntaxKind, lexeme: SmolStr },
    FinishNode,
}
