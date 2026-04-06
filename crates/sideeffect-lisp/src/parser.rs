use crate::types::Expr;

/// Parse an s-expression string into an Expr AST.
pub fn parse(input: &str) -> Result<Expr, String> {
    let tokens = tokenize(input)?;
    let mut pos = 0;
    let expr = parse_expr(&tokens, &mut pos)?;
    if pos < tokens.len() {
        // Wrap multiple top-level forms in a begin
        let mut forms = vec![expr];
        while pos < tokens.len() {
            forms.push(parse_expr(&tokens, &mut pos)?);
        }
        Ok(Expr::List(
            std::iter::once(Expr::Symbol("begin".into()))
                .chain(forms)
                .collect(),
        ))
    } else {
        Ok(expr)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    LParen,
    RParen,
    Quote,
    Atom(String),
}

fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' | '\n' | '\r' => { chars.next(); }
            ';' => {
                // Line comment
                while let Some(&c) = chars.peek() {
                    chars.next();
                    if c == '\n' { break; }
                }
            }
            '(' => { tokens.push(Token::LParen); chars.next(); }
            ')' => { tokens.push(Token::RParen); chars.next(); }
            '\'' => { tokens.push(Token::Quote); chars.next(); }
            '"' => {
                return Err("String literals not yet supported".into());
            }
            _ => {
                let mut atom = String::new();
                while let Some(&c) = chars.peek() {
                    if c == ' ' || c == '\t' || c == '\n' || c == '\r'
                        || c == '(' || c == ')' || c == ';'
                    {
                        break;
                    }
                    atom.push(c);
                    chars.next();
                }
                tokens.push(Token::Atom(atom));
            }
        }
    }
    Ok(tokens)
}

fn parse_expr(tokens: &[Token], pos: &mut usize) -> Result<Expr, String> {
    if *pos >= tokens.len() {
        return Err("Unexpected end of input".into());
    }
    match &tokens[*pos] {
        Token::LParen => {
            *pos += 1;
            let mut elems = Vec::new();
            loop {
                if *pos >= tokens.len() {
                    return Err("Unclosed parenthesis".into());
                }
                if tokens[*pos] == Token::RParen {
                    *pos += 1;
                    break;
                }
                elems.push(parse_expr(tokens, pos)?);
            }
            if elems.is_empty() {
                Ok(Expr::Nil)
            } else {
                Ok(Expr::List(elems))
            }
        }
        Token::RParen => Err("Unexpected ')'".into()),
        Token::Quote => {
            *pos += 1;
            let inner = parse_expr(tokens, pos)?;
            Ok(Expr::List(vec![Expr::Symbol("quote".into()), inner]))
        }
        Token::Atom(s) => {
            *pos += 1;
            parse_atom(s)
        }
    }
}

fn parse_atom(s: &str) -> Result<Expr, String> {
    if let Ok(n) = s.parse::<i32>() {
        return Ok(Expr::Int(n));
    }
    match s {
        "#t" | "true" => Ok(Expr::Bool(true)),
        "#f" | "false" => Ok(Expr::Bool(false)),
        "nil" => Ok(Expr::Nil),
        _ => Ok(Expr::Symbol(s.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_integer() {
        assert_eq!(parse("42").unwrap(), Expr::Int(42));
        assert_eq!(parse("-7").unwrap(), Expr::Int(-7));
    }

    #[test]
    fn test_parse_symbol() {
        assert_eq!(parse("foo").unwrap(), Expr::Symbol("foo".into()));
    }

    #[test]
    fn test_parse_nil() {
        assert_eq!(parse("()").unwrap(), Expr::Nil);
        assert_eq!(parse("nil").unwrap(), Expr::Nil);
    }

    #[test]
    fn test_parse_list() {
        assert_eq!(
            parse("(+ 1 2)").unwrap(),
            Expr::List(vec![
                Expr::Symbol("+".into()),
                Expr::Int(1),
                Expr::Int(2),
            ])
        );
    }

    #[test]
    fn test_parse_nested() {
        assert_eq!(
            parse("(+ (* 2 3) 4)").unwrap(),
            Expr::List(vec![
                Expr::Symbol("+".into()),
                Expr::List(vec![
                    Expr::Symbol("*".into()),
                    Expr::Int(2),
                    Expr::Int(3),
                ]),
                Expr::Int(4),
            ])
        );
    }

    #[test]
    fn test_parse_quote() {
        assert_eq!(
            parse("'foo").unwrap(),
            Expr::List(vec![
                Expr::Symbol("quote".into()),
                Expr::Symbol("foo".into()),
            ])
        );
    }

    #[test]
    fn test_parse_bool() {
        assert_eq!(parse("#t").unwrap(), Expr::Bool(true));
        assert_eq!(parse("#f").unwrap(), Expr::Bool(false));
    }

    #[test]
    fn test_parse_define_lambda() {
        let result = parse("(define (add a b) (+ a b))").unwrap();
        assert_eq!(
            result,
            Expr::List(vec![
                Expr::Symbol("define".into()),
                Expr::List(vec![
                    Expr::Symbol("add".into()),
                    Expr::Symbol("a".into()),
                    Expr::Symbol("b".into()),
                ]),
                Expr::List(vec![
                    Expr::Symbol("+".into()),
                    Expr::Symbol("a".into()),
                    Expr::Symbol("b".into()),
                ]),
            ])
        );
    }

    #[test]
    fn test_parse_comments() {
        assert_eq!(
            parse("; this is a comment\n42").unwrap(),
            Expr::Int(42)
        );
    }
}
