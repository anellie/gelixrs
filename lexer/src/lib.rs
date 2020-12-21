mod token;

use crate::token::{SToken, Token};
use logos::{Lexer, Logos};
use smol_str::SmolStr;
use std::{collections::VecDeque, convert::TryInto, iter::FromIterator, rc::Rc};

pub fn lex(source: &Rc<String>) -> VecDeque<SToken> {
    let mut out = VecDeque::with_capacity(300);
    for (tok, span) in &mut Token::lexer(source).spanned() {
        out.push_back(SToken::new(tok, span));
    }
    out
}

fn to_smol_str(lex: &mut Lexer<Token>) -> SmolStr {
    SmolStr::new(lex.slice())
}

fn string(lex: &mut Lexer<Token>) -> Result<SmolStr, &'static str> {
    let token = lex.slice();
    let str = &token[1..(token.len() - 1)]; // Trim '"'
    str_escape_seq(str)
}

/// Replace all escape sequences inside a string literal with their proper char
/// and return either an error or the finished string token
fn str_escape_seq(literal: &str) -> Result<SmolStr, &'static str> {
    let mut chars = literal.chars().collect::<Vec<_>>();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' {
            chars.remove(i);
            if chars.len() == i {
                return Err("Unfinished escape sequence at end of string.");
            }

            chars[i] = match chars[i] {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '\\' => '\\',
                '0' => '\0',
                '"' => '"',

                'u' => {
                    let mut esq_chars = Vec::with_capacity(6);
                    while chars.get(i + 1).map(char::is_ascii_hexdigit) == Some(true) {
                        esq_chars.push(chars.remove(i + 1));
                    }
                    u32::from_str_radix(&String::from_iter(esq_chars), 16)
                        .unwrap()
                        .try_into()
                        .unwrap()
                }

                _ => return Err("Unknown escape sequence."),
            }
        }
        i += 1;
    }
    Ok(SmolStr::from(chars.into_iter().collect::<String>()))
}
