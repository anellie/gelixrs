use std::{env, fs, fs::File, io::Write, path::PathBuf};

use serde::{Deserialize, Serialize};
use tera::{Context, Tera};

struct Exec {
    output: PathBuf,
    tera: Tera,
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
struct Node {
    #[serde(default = "default_template")]
    template: String,
    #[serde(default)]
    context: Ctx,
}

fn default_template() -> String {
    "basic.rs".to_string()
}

#[derive(Deserialize, Serialize, Default)]
struct Ctx {
    #[serde(default)]
    name: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    items: Vec<Item>,
}

#[derive(Deserialize, Serialize)]
struct Item {
    #[serde(default)]
    name: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    r#type: String,
    #[serde(default = "default_strategy")]
    strategy: String,
}

fn default_strategy() -> String {
    "single".to_string()
}

fn main() {
    let path = env::current_dir().unwrap();
    let parent = path.parent().unwrap();
    let mut output = PathBuf::from(parent);
    output.push("ast");
    output.push("src");
    output.push("generated_nodes.rs");

    let tera = match Tera::new("templates/*.rs") {
        Ok(t) => t,
        Err(e) => {
            println!("Parsing error(s): {}", e);
            std::process::exit(1);
        }
    };

    let mut nodes = env::current_dir().unwrap();
    nodes.push("nodes.ron");
    let nodes: Vec<Node> = ron::from_str(&fs::read_to_string("nodes.ron").unwrap()).unwrap();

    generate(Exec {
        output,
        tera,
        nodes,
    })
}

fn generate(e: Exec) {
    let mut out = String::with_capacity(10_000);

    for mut node in e.nodes {
        out.push('\n');
        process_context(&mut node.context);
        let ctx = Context::from_serialize(&node.context).unwrap();
        let rendered = e.tera.render(&node.template, &ctx).unwrap();
        out.push_str(&rendered);
        out.push('\n');
    }

    File::create(e.output)
        .unwrap()
        .write_all(out.as_bytes())
        .unwrap();
}

fn process_context(ctx: &mut Ctx) {
    if ctx.kind.is_empty() {
        ctx.kind = ctx.name.clone();
    }

    for item in &mut ctx.items {
        if item.kind.is_empty() {
            item.kind = item.r#type.clone();
        }

        let strat = item.strategy.clone();
        item.strategy = match &item.strategy[..] {
            "single" => format!("children().find_map({}::cast).unwrap()", item.r#type),
            "nested_single" => format!("children().find(|i| i.kind() == SyntaxKind::{}).unwrap().children().find_map({}::cast).unwrap()", item.kind, item.r#type),

            "opt_single" => format!("children().find_map({}::cast)", item.r#type),
            "nested_opt_single" => format!(
                "children().find(|i| i.kind() == SyntaxKind::{}).map(|i| i.children().find_map({}::cast)).flatten()",
                item.kind, item.r#type
            ),

            "list" => format!("children().filter_map({}::cast)", item.r#type),
            "nested_list" => format!(
                "children().filter(|i| i.kind() == SyntaxKind::{}).map(|i| i.children().find_map({}::cast).unwrap())",
                item.kind, item.r#type
            ),

            "token" => "children_with_tokens()\
            .find(|c| c.as_token().map(Token::kind).as_ref().map(SyntaxKind::is_token) == Some(true))\
            .unwrap().as_token().unwrap().kind()".to_string(),
            "nested_token" => format!("children().find(|i| i.kind() == SyntaxKind::{}).unwrap().children_with_tokens()\
            .find(|c| c.as_token().map(Token::kind).as_ref().map(SyntaxKind::is_token) == Some(true))\
            .unwrap().as_token().unwrap().kind()", item.kind),
            "nested_token_list" => format!("children().filter(|i| i.kind() == SyntaxKind::{}).map(|c| c.children_with_tokens()\
            .find(|c| c.as_token().map(Token::kind).as_ref().map(SyntaxKind::is_token) == Some(true)))\
            .flatten().map(|c| c.as_token().unwrap().kind())", item.kind),

            "ident" => "children_with_tokens()\
            .find(|c| c.as_token().map(Token::kind) == Some(SyntaxKind::Identifier))\
            .unwrap().as_token().unwrap().text().clone()".to_string(),
            "ident_list" => "children_with_tokens()\
            .filter(|c| c.as_token().map(Token::kind) == Some(SyntaxKind::Identifier))\
            .map(|c| c.as_token().unwrap().text().clone())".to_string(),

            _ => item.strategy.clone()
        };

        item.r#type = match &strat[..] {
            "opt_single" | "nested_opt_single" => format!("Option<{}>", item.r#type),
            "list" | "nested_list" => format!("impl Iterator<Item = {}> + '_", item.r#type),
            "token" | "nested_token" => "SyntaxKind".to_string(),
            "nested_token_list" => "impl Iterator<Item = SyntaxKind> + '_".to_string(),
            "ident" => "SmolStr".to_string(),
            "ident_list" => "impl Iterator<Item = SmolStr> + '_".to_string(),
            _ => item.r#type.clone(),
        }
    }
}
