//! RFC 5322 email address parser using chumsky.
//!
//! This module provides strict RFC 5322-compliant parsing of email addresses (addr-spec).
//! It handles all RFC 5322 features including:
//! - Dot-atom local parts
//! - Quoted-string local parts
//! - Domain names (dot-atom)
//! - Domain literals (IP addresses in brackets)
//! - Comments (CFWS)
//! - Folding whitespace (FWS)
//! - Quoted-pairs
//! - Obsolete syntax

use chumsky::prelude::*;

use crate::parser::ParseError;

/// Parse an RFC 5322 email address (addr-spec).
///
/// addr-spec = local-part "@" domain
///
/// Returns the parsed email address as a String, normalized according to RFC 5322.
/// Comments (CFWS) are removed, but quoted strings, domain literals, and other
/// RFC-compliant features are preserved.
///
/// Leading and trailing CFWS (comments and folding whitespace) are handled per RFC 5322.
/// Empty strings or strings containing only CFWS will result in an error.
pub fn parse_email_address(value: &str) -> Result<String, ParseError> {
    let parser = addr_spec_parser();

    parser
        .parse(value)
        .into_result()
        .map_err(|_| ParseError::InvalidEmail(value.to_string()))
}

/// RFC 5322 addr-spec parser (internal, without end check).
///
/// This is used when addr-spec appears as part of a larger structure (e.g., angle-addr).
/// For standalone addr-spec parsing, use parse_email_address() which includes end() check.
pub fn addr_spec_parser_internal<'src>(
) -> impl Parser<'src, &'src str, String, extra::Err<Rich<'src, char>>> {
    cfws()
        .or_not()
        .ignore_then(
            local_part_parser()
                .then_ignore(just('@'))
                .then(domain_parser())
                .map(|(local, domain)| format!("{}@{}", local, domain)),
        )
        .then_ignore(cfws().or_not())
        .labelled("addr-spec")
}

/// RFC 5322 addr-spec parser.
///
/// addr-spec = local-part "@" domain
///
/// Note: When addr-spec appears in mailbox specifications, it can be surrounded by CFWS.
/// We handle optional CFWS at the start and end for RFC compliance.
///
/// The parser ensures the entire input is consumed (no trailing garbage allowed).
fn addr_spec_parser<'src>() -> impl Parser<'src, &'src str, String, extra::Err<Rich<'src, char>>> {
    addr_spec_parser_internal().then_ignore(end())
}

/// RFC 5322 local-part parser.
///
/// local-part = dot-atom / quoted-string / obs-local-part
/// Note: obs-local-part must be tried before quoted-string because it can start with a quoted-string
fn local_part_parser<'src>() -> impl Parser<'src, &'src str, String, extra::Err<Rich<'src, char>>> {
    choice((
        dot_atom_parser(),
        obs_local_part_parser(),
        quoted_string_parser(),
    ))
    .labelled("local-part")
}

/// RFC 5322 domain parser.
///
/// domain = dot-atom / domain-literal / obs-domain
/// Note: obs-domain must be tried before dot-atom because it can contain CFWS that dot-atom would also match
fn domain_parser<'src>() -> impl Parser<'src, &'src str, String, extra::Err<Rich<'src, char>>> {
    choice((
        domain_literal_parser(),
        obs_domain_parser(),
        dot_atom_parser(),
    ))
    .labelled("domain")
}

/// RFC 5322 dot-atom parser.
///
/// dot-atom = [CFWS] dot-atom-text [CFWS]
fn dot_atom_parser<'src>() -> impl Parser<'src, &'src str, String, extra::Err<Rich<'src, char>>> {
    cfws()
        .or_not()
        .ignore_then(dot_atom_text_parser())
        .then_ignore(cfws().or_not())
        .labelled("dot-atom")
}

/// RFC 5322 dot-atom-text parser.
///
/// dot-atom-text = 1*atext *("." 1*atext)
fn dot_atom_text_parser<'src>() -> impl Parser<'src, &'src str, String, extra::Err<Rich<'src, char>>>
{
    atext()
        .repeated()
        .at_least(1)
        .collect::<String>()
        .then(
            just('.')
                .ignore_then(atext().repeated().at_least(1).collect::<String>())
                .repeated()
                .collect::<Vec<String>>(),
        )
        .map(|(first, rest)| {
            let mut result = first;
            for part in rest {
                result.push('.');
                result.push_str(&part);
            }
            result
        })
        .labelled("dot-atom-text")
}

/// RFC 5322 atext parser.
///
/// atext = ALPHA / DIGIT / "!" / "#" / "$" / "%" / "&" / "'" / "*" / "+" / "-" / "/" / "=" / "?" / "^" / "_" / "`" / "{" / "|" / "}" / "~"
/// Note: RFC 5322 is ASCII-only, so we only accept ASCII letters and digits
fn atext<'src>() -> impl Parser<'src, &'src str, char, extra::Err<Rich<'src, char>>> {
    any()
        .filter(|c: &char| {
            // Only ASCII alphanumeric (not Unicode)
            ((*c as u32) >= 65 && (*c as u32) <= 90)  // A-Z
                || ((*c as u32) >= 97 && (*c as u32) <= 122)  // a-z
                || ((*c as u32) >= 48 && (*c as u32) <= 57)  // 0-9
                || matches!(
                    c,
                    '!' | '#'
                        | '$'
                        | '%'
                        | '&'
                        | '\''
                        | '*'
                        | '+'
                        | '-'
                        | '/'
                        | '='
                        | '?'
                        | '^'
                        | '_'
                        | '`'
                        | '{'
                        | '|'
                        | '}'
                        | '~'
                )
        })
        .labelled("atext")
}

/// RFC 5322 quoted-string parser.
///
/// quoted-string = [CFWS] DQUOTE *([FWS] qcontent) [FWS] DQUOTE [CFWS]
/// FWS inside quoted strings should be preserved as spaces (normalized from CRLF+WSP)
fn quoted_string_parser<'src>() -> impl Parser<'src, &'src str, String, extra::Err<Rich<'src, char>>>
{
    cfws()
        .or_not()
        .ignore_then(just('"'))
        .ignore_then(
            // *([FWS] qcontent)
            // Parse sequence of: optional FWS (normalized to space), then qcontent
            // FWS can be: WSP, CRLF+WSP, etc. We normalize all to a single space
            // Note: * means zero or more, so empty quoted string is valid
            choice((
                // FWS followed by qcontent - FWS is preserved as string
                fws_as_space()
                    .then(qcontent())
                    .map(|(fws_str, content)| format!("{}{}", fws_str, content)),
                // Just qcontent (no FWS)
                qcontent(),
            ))
            .repeated()
            .collect::<Vec<String>>()
            .map(|parts| parts.join("")),
        )
        .then(
            // [FWS] before closing quote - preserve as string
            fws_as_space().or_not(),
        )
        .then_ignore(just('"'))
        .then_ignore(cfws().or_not())
        .map(|(content, trailing_fws)| {
            let mut result = String::from("\"");
            result.push_str(&content);
            if let Some(fws_str) = trailing_fws {
                result.push_str(&fws_str);
            }
            result.push('"');
            result
        })
        .labelled("quoted-string")
}

/// Helper: Parse FWS and preserve whitespace characters (for use in quoted strings).
/// According to RFC 5322, FWS inside quoted strings should be preserved.
/// CRLF+WSP is normalized to a single space, but WSP characters (spaces/tabs) are preserved as-is.
fn fws_as_space<'src>() -> impl Parser<'src, &'src str, String, extra::Err<Rich<'src, char>>> {
    choice((
        // Standard FWS: ([*WSP CRLF] 1*WSP)
        // Parse: optional leading WSP, then zero or more (CRLF + WSP), then trailing WSP
        // CRLF+WSP sequences become a single space, but standalone WSP is preserved
        wsp()
            .repeated()
            .collect::<String>()
            .then(
                just('\r')
                    .ignore_then(just('\n'))
                    .ignore_then(wsp().repeated().at_least(1).collect::<String>())
                    .map(|_| " ".to_string()) // CRLF+WSP -> single space
                    .repeated()
                    .collect::<Vec<String>>()
                    .map(|parts| parts.join("")),
            )
            .then(wsp().repeated().collect::<String>())
            .map(|((leading, crlf_normalized), trailing)| {
                let mut result = leading;
                result.push_str(&crlf_normalized);
                result.push_str(&trailing);
                result
            }),
        // obs-FWS: 1*WSP *(CRLF 1*WSP)
        wsp()
            .repeated()
            .at_least(1)
            .collect::<String>()
            .then(
                just('\r')
                    .ignore_then(just('\n'))
                    .ignore_then(wsp().repeated().at_least(1).collect::<String>())
                    .map(|_| " ".to_string()) // CRLF+WSP -> single space
                    .repeated()
                    .collect::<Vec<String>>()
                    .map(|parts| parts.join("")),
            )
            .map(|(leading, crlf_normalized)| format!("{}{}", leading, crlf_normalized)),
        // Simplified: just whitespace - preserve as-is
        wsp().repeated().at_least(1).collect::<String>(),
    ))
    .labelled("FWS-preserve")
}

/// RFC 5322 qcontent parser.
///
/// qcontent = qtext / quoted-pair
/// Note: quoted-pair must be tried first because it starts with '\' which qtext excludes
fn qcontent<'src>() -> impl Parser<'src, &'src str, String, extra::Err<Rich<'src, char>>> {
    choice((quoted_pair(), qtext().map(|c| c.to_string()))).labelled("qcontent")
}

/// RFC 5322 qtext parser.
///
/// qtext = %d33 / %d35-91 / %d93-126
/// This is any character except '"' and '\' (which are handled by quoted-pair)
fn qtext<'src>() -> impl Parser<'src, &'src str, char, extra::Err<Rich<'src, char>>> {
    any()
        .filter(|c: &char| {
            let code = *c as u32;
            *c != '"' && *c != '\\' && (33..=126).contains(&code)
        })
        .labelled("qtext")
}

/// RFC 5322 quoted-pair parser.
///
/// quoted-pair = "\" (VCHAR / WSP)
/// Returns the escaped character (preserving the backslash in output for quoted strings)
/// Uses one_of() to create a cloneable parser for use in recursive structures.
fn quoted_pair<'src>() -> impl Parser<'src, &'src str, String, extra::Err<Rich<'src, char>>> + Clone
{
    // Build character set: VCHAR (33-126) + WSP (space, tab)
    let mut chars = String::new();
    chars.push(' '); // SP
    chars.push('\t'); // HTAB
    for code in 33..=126u32 {
        chars.push(char::from_u32(code).unwrap());
    }
    just('\\')
        .ignore_then(one_of(chars))
        .map(|c| format!("\\{}", c))
        .labelled("quoted-pair")
}

/// RFC 5322 domain-literal parser.
///
/// domain-literal = [CFWS] "[" *([FWS] dtext) [FWS] "]" [CFWS]
fn domain_literal_parser<'src>(
) -> impl Parser<'src, &'src str, String, extra::Err<Rich<'src, char>>> {
    cfws()
        .or_not()
        .ignore_then(just('['))
        .then(
            // *([FWS] dtext)
            // Parse zero or more of: optional FWS, then one or more dtext items
            // FWS should be normalized to space like in quoted strings
            // dtext can be a single character or a quoted-pair
            fws_as_space()
                .or_not()
                .then(
                    // Parse one or more dtext items (characters or quoted-pairs) in sequence
                    dtext()
                        .repeated()
                        .at_least(1)
                        .collect::<Vec<String>>()
                        .map(|parts| parts.join("")),
                )
                .map(|(fws_opt, dtext_content)| {
                    if let Some(fws_str) = fws_opt {
                        format!("{}{}", fws_str, dtext_content)
                    } else {
                        dtext_content
                    }
                })
                .repeated()
                .collect::<Vec<String>>()
                .map(|parts| parts.join("")),
        )
        .then(fws().or_not())
        .then_ignore(just(']'))
        .then_ignore(cfws().or_not())
        .map(|((_, content), _)| format!("[{}]", content))
        .labelled("domain-literal")
}

/// RFC 5322 dtext parser (for use in domain-literal).
///
/// dtext = %d33-90 / %d94-126 / obs-dtext
/// obs-dtext = obs-NO-WS-CTL / quoted-pair
/// This returns a String containing the dtext content, preserving quoted-pairs as "\X"
fn dtext<'src>() -> impl Parser<'src, &'src str, String, extra::Err<Rich<'src, char>>> {
    choice((
        // obs-dtext: quoted-pair (allows quoted-pairs in domain-literal)
        // Must be first to handle backslash before regular dtext
        quoted_pair(),
        // Standard dtext: %d33-90 / %d94-126
        any()
            .filter(|c: &char| {
                let code = *c as u32;
                (33..=90).contains(&code) || (94..=126).contains(&code)
            })
            .map(|c| c.to_string()),
    ))
    .labelled("dtext")
}

/// RFC 5322 CFWS (comments and folding whitespace) parser.
///
/// CFWS = (1*([FWS] comment) [FWS]) / FWS
/// This means: one or more of (optional FWS, then comment), then optional FWS, OR just FWS
fn cfws<'src>() -> impl Parser<'src, &'src str, (), extra::Err<Rich<'src, char>>> {
    choice((
        // (1*([FWS] comment) [FWS])
        // One or more of: optional FWS, then comment
        fws()
            .or_not()
            .ignore_then(comment())
            .then(fws().or_not())
            .repeated()
            .at_least(1)
            .then(fws().or_not())
            .ignored(),
        // FWS
        fws(),
    ))
    .labelled("CFWS")
}

/// RFC 5322 FWS (folding whitespace) parser.
///
/// FWS = ([*WSP CRLF] 1*WSP) / obs-FWS
fn fws<'src>() -> impl Parser<'src, &'src str, (), extra::Err<Rich<'src, char>>> {
    choice((
        // Standard FWS: ([*WSP CRLF] 1*WSP)
        wsp()
            .repeated()
            .then(
                just('\r')
                    .ignore_then(just('\n'))
                    .ignore_then(wsp().repeated().at_least(1))
                    .repeated(),
            )
            .then(wsp().repeated().at_least(1))
            .ignored(),
        // obs-FWS: 1*WSP *(CRLF 1*WSP)
        obs_fws(),
        // Simplified: just whitespace (common case)
        wsp().repeated().at_least(1).ignored(),
    ))
    .labelled("FWS")
}

/// RFC 5322 WSP (whitespace) parser.
///
/// WSP = SP / HTAB
/// Uses one_of() to create a cloneable parser for use in recursive structures.
fn wsp<'src>() -> impl Parser<'src, &'src str, char, extra::Err<Rich<'src, char>>> + Clone {
    one_of(" \t").labelled("WSP")
}

/// RFC 5322 comment parser.
///
/// comment = "(" *([FWS] ccontent) [FWS] ")"
/// ccontent = ctext / quoted-pair / comment
///
/// This implementation uses recursive() to handle nested comments properly.
/// By making ctext(), quoted_pair(), and wsp() cloneable using one_of(),
/// we can use them inside recursive() without issues.
fn comment<'src>() -> impl Parser<'src, &'src str, (), extra::Err<Rich<'src, char>>> {
    recursive(|nested_comment| {
        // Simple whitespace parser for use inside comments (cloneable)
        let simple_fws = wsp().repeated().at_least(1).ignored();

        // Build ccontent parser - now cloneable!
        let ccontent = choice((
            ctext().map(|_| ()),
            quoted_pair().map(|_| ()),
            nested_comment.clone(),
        ));

        // Parse the comment structure: ( *([FWS] ccontent) [FWS] )
        // This means: opening paren, zero or more of (optional FWS then ccontent), optional FWS, closing paren
        just('(')
            .ignore_then(
                // *([FWS] ccontent) - zero or more of: optional FWS, then ccontent
                simple_fws
                    .clone()
                    .or_not()
                    .ignore_then(ccontent)
                    .repeated()
                    .collect::<Vec<_>>()
                    // [FWS] - optional trailing FWS
                    .then(simple_fws.clone().or_not()),
            )
            .then_ignore(just(')'))
            .ignored()
            .labelled("comment")
    })
}

/// RFC 5322 ctext parser.
///
/// ctext = %d33-39 / %d42-91 / %d93-126
/// This is any character except '(', ')', and '\' (which are handled by quoted-pair and comment)
///
/// Uses one_of() with character ranges to create a cloneable parser for use in recursive structures.
fn ctext<'src>() -> impl Parser<'src, &'src str, char, extra::Err<Rich<'src, char>>> + Clone {
    // Build character set: all printable ASCII (33-126) except '(', ')', '\'
    let mut chars = String::new();
    for code in 33..=126u32 {
        let c = char::from_u32(code).unwrap();
        if c != '(' && c != ')' && c != '\\' {
            chars.push(c);
        }
    }
    one_of(chars).labelled("ctext")
}

/// RFC 5322 obsolete local-part parser.
///
/// obs-local-part = word *("." word)
/// word = atom / quoted-string
fn obs_local_part_parser<'src>(
) -> impl Parser<'src, &'src str, String, extra::Err<Rich<'src, char>>> {
    obs_word()
        .then(
            just('.')
                .ignore_then(obs_word())
                .repeated()
                .collect::<Vec<String>>(),
        )
        .map(|(first, rest)| {
            let mut result = first;
            for part in rest {
                result.push('.');
                result.push_str(&part);
            }
            result
        })
        .labelled("obs-local-part")
}

/// RFC 5322 obsolete word parser.
///
/// word = atom / quoted-string
fn obs_word<'src>() -> impl Parser<'src, &'src str, String, extra::Err<Rich<'src, char>>> {
    choice((atom(), quoted_string_parser())).labelled("obs-word")
}

/// RFC 5322 atom parser (for obsolete syntax).
///
/// atom = [CFWS] 1*atext [CFWS]
fn atom<'src>() -> impl Parser<'src, &'src str, String, extra::Err<Rich<'src, char>>> {
    cfws()
        .or_not()
        .ignore_then(atext().repeated().at_least(1).collect::<String>())
        .then_ignore(cfws().or_not())
        .labelled("atom")
}

/// RFC 5322 obsolete domain parser.
///
/// obs-domain = atom *("." atom)
fn obs_domain_parser<'src>() -> impl Parser<'src, &'src str, String, extra::Err<Rich<'src, char>>> {
    atom()
        .then(
            just('.')
                .ignore_then(atom())
                .repeated()
                .collect::<Vec<String>>(),
        )
        .map(|(first, rest)| {
            let mut result = first;
            for part in rest {
                result.push('.');
                result.push_str(&part);
            }
            result
        })
        .labelled("obs-domain")
}

/// RFC 5322 obsolete FWS parser.
///
/// obs-FWS = 1*WSP *(CRLF 1*WSP)
fn obs_fws<'src>() -> impl Parser<'src, &'src str, (), extra::Err<Rich<'src, char>>> {
    wsp()
        .repeated()
        .at_least(1)
        .then(
            just('\r')
                .ignore_then(just('\n'))
                .ignore_then(wsp().repeated().at_least(1))
                .repeated()
                .collect::<Vec<_>>(),
        )
        .ignored()
        .labelled("obs-FWS")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_addr_spec() {
        let result = parse_email_address("user@example.com").unwrap();
        assert_eq!(result, "user@example.com");
    }

    #[test]
    fn test_dot_atom_local_part() {
        let result = parse_email_address("user.name@example.com").unwrap();
        assert_eq!(result, "user.name@example.com");
    }

    #[test]
    fn test_quoted_local_part() {
        let result = parse_email_address("\"user name\"@example.com").unwrap();
        assert_eq!(result, "\"user name\"@example.com");
    }

    #[test]
    fn test_quoted_local_part_with_special_chars() {
        let result = parse_email_address("\"user+name\"@example.com").unwrap();
        assert_eq!(result, "\"user+name\"@example.com");
    }

    #[test]
    fn test_quoted_pair_in_local_part() {
        let result = parse_email_address("\"user\\\"name\"@example.com").unwrap();
        assert_eq!(result, "\"user\\\"name\"@example.com");
    }

    #[test]
    fn test_domain_literal() {
        let result = parse_email_address("user@[192.168.1.1]").unwrap();
        assert_eq!(result, "user@[192.168.1.1]");
    }

    #[test]
    fn test_domain_literal_with_quoted_pair() {
        let result = parse_email_address("user@[192\\.168\\.1\\.1]").unwrap();
        assert_eq!(result, "user@[192\\.168\\.1\\.1]");
    }

    #[test]
    fn test_with_comments() {
        let result = parse_email_address("user(comment)@example.com").unwrap();
        assert_eq!(result, "user@example.com");
    }

    #[test]
    fn test_with_comments_both_sides() {
        let result =
            parse_email_address("(comment)user(comment)@(comment)example.com(comment)").unwrap();
        assert_eq!(result, "user@example.com");
    }

    #[test]
    fn test_nested_comments() {
        let result = parse_email_address("user(outer(inner)outer)@example.com").unwrap();
        assert_eq!(result, "user@example.com");
    }

    #[test]
    fn test_deeply_nested_comments() {
        let result = parse_email_address("user(a(b(c)d)e)@example.com").unwrap();
        assert_eq!(result, "user@example.com");
    }

    #[test]
    fn test_nested_comments_with_text() {
        let result = parse_email_address("user(text(inner)text)@example.com").unwrap();
        assert_eq!(result, "user@example.com");
    }

    #[test]
    fn test_multiple_nested_comments() {
        let result = parse_email_address("user(a(b)c(d)e)@example.com").unwrap();
        assert_eq!(result, "user@example.com");
    }

    #[test]
    fn test_empty_nested_comment() {
        let result = parse_email_address("user(())@example.com").unwrap();
        assert_eq!(result, "user@example.com");
    }

    #[test]
    fn test_triple_nested_comments() {
        let result = parse_email_address("user(a(b(c)d)e)@example.com").unwrap();
        assert_eq!(result, "user@example.com");
    }

    #[test]
    fn test_quoted_pair_in_comment() {
        let result = parse_email_address("user(comment\\)with\\)escapes)@example.com").unwrap();
        assert_eq!(result, "user@example.com");
    }

    #[test]
    fn test_folding_whitespace() {
        let result = parse_email_address("user \r\n @example.com").unwrap();
        assert_eq!(result, "user@example.com");
    }

    #[test]
    fn test_special_chars_in_local_part() {
        let result = parse_email_address("user+tag@example.com").unwrap();
        assert_eq!(result, "user+tag@example.com");
    }

    #[test]
    fn test_all_atext_chars() {
        // Note: " and \ need to be quoted
        let result = parse_email_address("!#$%&'*+-/=?^_`{|}~@example.com").unwrap();
        assert_eq!(result, "!#$%&'*+-/=?^_`{|}~@example.com");
    }

    #[test]
    fn test_quoted_string_with_fws() {
        let result = parse_email_address("\"user name\"@example.com").unwrap();
        assert_eq!(result, "\"user name\"@example.com");
    }

    #[test]
    fn test_quoted_string_with_quoted_pair() {
        let result = parse_email_address("\"user\\\\name\"@example.com").unwrap();
        assert_eq!(result, "\"user\\\\name\"@example.com");
    }

    #[test]
    fn test_invalid_empty() {
        assert!(parse_email_address("").is_err());
    }

    #[test]
    fn test_invalid_no_at() {
        assert!(parse_email_address("userexample.com").is_err());
    }

    #[test]
    fn test_invalid_no_local_part() {
        assert!(parse_email_address("@example.com").is_err());
    }

    #[test]
    fn test_invalid_no_domain() {
        assert!(parse_email_address("user@").is_err());
    }

    #[test]
    fn test_quoted_string_empty() {
        let result = parse_email_address("\"\"@example.com").unwrap();
        assert_eq!(result, "\"\"@example.com");
    }

    #[test]
    fn test_domain_literal_empty() {
        let result = parse_email_address("user@[]").unwrap();
        assert_eq!(result, "user@[]");
    }

    // RFC 5322 Comprehensive Compliance Tests

    #[test]
    fn test_rfc5322_quoted_string_preserves_spaces() {
        // RFC 5322: Spaces inside quoted strings must be preserved
        let result = parse_email_address("\"user name\"@example.com").unwrap();
        assert_eq!(result, "\"user name\"@example.com");
    }

    #[test]
    fn test_rfc5322_quoted_string_with_multiple_spaces() {
        let result = parse_email_address("\"user  name\"@example.com").unwrap();
        assert_eq!(result, "\"user  name\"@example.com");
    }

    #[test]
    fn test_rfc5322_quoted_string_with_tab() {
        // Tab is WSP and should be preserved in quoted strings
        let result = parse_email_address("\"user\tname\"@example.com").unwrap();
        assert_eq!(result, "\"user\tname\"@example.com");
    }

    #[test]
    fn test_rfc5322_quoted_string_with_fws_crlf() {
        // FWS with CRLF should be normalized to space
        let result = parse_email_address("\"user\r\n name\"@example.com").unwrap();
        assert_eq!(result, "\"user name\"@example.com");
    }

    #[test]
    fn test_rfc5322_quoted_pair_quote() {
        // Escaped quote should be preserved
        let result = parse_email_address("\"user\\\"name\"@example.com").unwrap();
        assert_eq!(result, "\"user\\\"name\"@example.com");
    }

    #[test]
    fn test_rfc5322_quoted_pair_backslash() {
        // Escaped backslash should be preserved
        let result = parse_email_address("\"user\\\\name\"@example.com").unwrap();
        assert_eq!(result, "\"user\\\\name\"@example.com");
    }

    #[test]
    fn test_rfc5322_quoted_pair_space() {
        // Escaped space should be preserved
        let result = parse_email_address("\"user\\ name\"@example.com").unwrap();
        assert_eq!(result, "\"user\\ name\"@example.com");
    }

    #[test]
    fn test_rfc5322_quoted_pair_tab() {
        // Escaped tab should be preserved
        let result = parse_email_address("\"user\\\tname\"@example.com").unwrap();
        assert_eq!(result, "\"user\\\tname\"@example.com");
    }

    #[test]
    fn test_rfc5322_quoted_pair_vchar() {
        // Escaped printable character should be preserved
        let result = parse_email_address("\"user\\@name\"@example.com").unwrap();
        assert_eq!(result, "\"user\\@name\"@example.com");
    }

    #[test]
    fn test_rfc5322_domain_literal_preserves_content() {
        // Domain literal should preserve all dtext
        let result = parse_email_address("user@[192.168.1.1]").unwrap();
        assert_eq!(result, "user@[192.168.1.1]");
    }

    #[test]
    fn test_rfc5322_domain_literal_quoted_pair() {
        // Quoted pairs in domain literal should be preserved
        let result = parse_email_address("user@[192\\.168\\.1\\.1]").unwrap();
        assert_eq!(result, "user@[192\\.168\\.1\\.1]");
    }

    #[test]
    fn test_rfc5322_domain_literal_with_brackets_escaped() {
        // Brackets can be escaped in domain literal
        let result = parse_email_address("user@[\\[test\\]]").unwrap();
        assert_eq!(result, "user@[\\[test\\]]");
    }

    #[test]
    fn test_rfc5322_dot_atom_leading_dot_invalid() {
        // Dot-atom cannot start with a dot
        assert!(parse_email_address(".user@example.com").is_err());
    }

    #[test]
    fn test_rfc5322_dot_atom_trailing_dot_invalid() {
        // Dot-atom cannot end with a dot
        assert!(parse_email_address("user.@example.com").is_err());
    }

    #[test]
    fn test_rfc5322_dot_atom_consecutive_dots_invalid() {
        // Dot-atom cannot have consecutive dots
        assert!(parse_email_address("user..name@example.com").is_err());
    }

    #[test]
    fn test_rfc5322_dot_atom_valid_multiple_dots() {
        // Valid dot-atom with multiple dots
        let result = parse_email_address("user.name.test@example.com").unwrap();
        assert_eq!(result, "user.name.test@example.com");
    }

    #[test]
    fn test_rfc5322_local_part_max_length() {
        // RFC 5321: Local part can be up to 64 octets
        let long_local = "a".repeat(64);
        let result = parse_email_address(&format!("{}@example.com", long_local)).unwrap();
        assert_eq!(result, format!("{}@example.com", long_local));
    }

    #[test]
    fn test_rfc5322_domain_max_length() {
        // RFC 5321: Domain can be up to 255 octets
        let long_domain = format!("{}.com", "a".repeat(250));
        let result = parse_email_address(&format!("user@{}", long_domain)).unwrap();
        assert_eq!(result, format!("user@{}", long_domain));
    }

    #[test]
    fn test_rfc5322_quoted_string_all_qtext_chars() {
        // Test all valid qtext characters (33, 35-91, 93-126)
        // Note: " and \ must be escaped as quoted-pairs: \" becomes \\" in Rust source, \ becomes \\
        let result = parse_email_address("\"!\\\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\\\]^_`abcdefghijklmnopqrstuvwxyz{|}~\"@example.com").unwrap();
        // Note: qtext excludes " and \ (handled by quoted-pair)
        assert!(result.contains("example.com"));
    }

    #[test]
    fn test_rfc5322_cfws_before_local_part() {
        // CFWS before local part should be ignored
        let result = parse_email_address(" user@example.com").unwrap();
        assert_eq!(result, "user@example.com");
    }

    #[test]
    fn test_rfc5322_cfws_after_domain() {
        // CFWS after domain should be ignored
        let result = parse_email_address("user@example.com ").unwrap();
        assert_eq!(result, "user@example.com");
    }

    #[test]
    fn test_rfc5322_cfws_with_comments() {
        // CFWS with comments should be ignored
        let result = parse_email_address("user(comment)@example.com").unwrap();
        assert_eq!(result, "user@example.com");
    }

    #[test]
    fn test_rfc5322_obs_local_part() {
        // Obsolete local-part syntax (word *("." word))
        let result = parse_email_address("\"user\".name@example.com").unwrap();
        assert_eq!(result, "\"user\".name@example.com");
    }

    #[test]
    fn test_rfc5322_obs_domain() {
        // Obsolete domain syntax (atom *("." atom))
        let result = parse_email_address("user@(comment)example(comment).com").unwrap();
        assert_eq!(result, "user@example.com");
    }

    #[test]
    fn test_rfc5322_quoted_string_with_comments() {
        // Comments in CFWS around quoted string
        let result = parse_email_address("\"user\"(comment)@example.com").unwrap();
        assert_eq!(result, "\"user\"@example.com");
    }

    #[test]
    fn test_rfc5322_invalid_quoted_string_unterminated() {
        // Unterminated quoted string should fail
        assert!(parse_email_address("\"user@example.com").is_err());
    }

    #[test]
    fn test_rfc5322_invalid_domain_literal_unterminated() {
        // Unterminated domain literal should fail
        assert!(parse_email_address("user@[192.168.1.1").is_err());
    }

    #[test]
    fn test_rfc5322_invalid_comment_unterminated() {
        // Unterminated comment should fail
        assert!(parse_email_address("user(comment@example.com").is_err());
    }

    #[test]
    fn test_rfc5322_quoted_string_empty_allowed() {
        // Empty quoted string is allowed per RFC 5322 (though erratum suggests otherwise)
        let result = parse_email_address("\"\"@example.com").unwrap();
        assert_eq!(result, "\"\"@example.com");
    }

    #[test]
    fn test_rfc5322_quoted_string_only_fws() {
        // Quoted string with only FWS
        let result = parse_email_address("\" \"@example.com").unwrap();
        assert_eq!(result.as_str(), "\" \"@example.com");
    }

    #[test]
    fn test_rfc5322_quoted_string_fws_before_close() {
        // FWS before closing quote
        let result = parse_email_address("\"user \"@example.com").unwrap();
        assert_eq!(result.as_str(), "\"user \"@example.com");
    }

    #[test]
    fn test_rfc5322_quoted_string_fws_after_open() {
        // FWS after opening quote
        let result = parse_email_address("\" user\"@example.com").unwrap();
        assert_eq!(result.as_str(), "\" user\"@example.com");
    }

    #[test]
    fn test_rfc5322_complex_quoted_string() {
        // Complex quoted string with mixed content
        let result = parse_email_address("\"user\\\"name\\\"test\"@example.com").unwrap();
        assert_eq!(result, "\"user\\\"name\\\"test\"@example.com");
    }

    #[test]
    fn test_rfc5322_domain_literal_ipv4() {
        // IPv4 address in domain literal
        let result = parse_email_address("user@[127.0.0.1]").unwrap();
        assert_eq!(result, "user@[127.0.0.1]");
    }

    #[test]
    fn test_rfc5322_domain_literal_ipv6_format() {
        // IPv6-like format in domain literal (RFC 5322 doesn't specify IPv6, but allows any dtext)
        let result = parse_email_address("user@[2001:db8::1]").unwrap();
        assert_eq!(result, "user@[2001:db8::1]");
    }

    #[test]
    fn test_rfc5322_atext_all_special_chars() {
        // All atext special characters
        let result = parse_email_address("!#$%&'*+-/=?^_`{|}~@example.com").unwrap();
        assert_eq!(result, "!#$%&'*+-/=?^_`{|}~@example.com");
    }

    #[test]
    fn test_rfc5322_local_part_case_sensitive() {
        // Local part is case-sensitive per RFC 5321
        let result1 = parse_email_address("User@example.com").unwrap();
        let result2 = parse_email_address("user@example.com").unwrap();
        assert_ne!(result1, result2);
    }

    #[test]
    fn test_rfc5322_domain_case_insensitive() {
        // Domain is case-insensitive, but parser should preserve case
        let result = parse_email_address("user@Example.COM").unwrap();
        // Parser should preserve case
        assert!(result.contains("@"));
    }

    // Additional comprehensive RFC 5322 compliance tests

    #[test]
    fn test_rfc5322_quoted_string_with_multiple_quoted_pairs() {
        // Multiple quoted-pairs in sequence
        let result = parse_email_address("\"user\\\"\\@name\"@example.com").unwrap();
        assert_eq!(result, "\"user\\\"\\@name\"@example.com");
    }

    #[test]
    fn test_rfc5322_quoted_string_with_crlf_fws() {
        // CRLF followed by whitespace should be normalized to space
        let result = parse_email_address("\"user\r\n name\"@example.com").unwrap();
        assert_eq!(result, "\"user name\"@example.com");
    }

    #[test]
    fn test_rfc5322_quoted_string_with_tab_fws() {
        // Tab should be preserved in quoted strings
        let result = parse_email_address("\"user\t\tname\"@example.com").unwrap();
        assert_eq!(result, "\"user\t\tname\"@example.com");
    }

    #[test]
    fn test_rfc5322_domain_literal_with_fws() {
        // FWS in domain literal should be normalized
        // Note: Leading space + CRLF + space becomes: space + normalized space = two spaces
        // This is correct per RFC 5322 FWS normalization
        let result = parse_email_address("user@[192 \r\n .168.1.1]").unwrap();
        // The FWS normalization preserves leading WSP, then CRLF+WSP becomes space
        assert!(result.contains("@[192"));
        assert!(result.contains(".168.1.1]"));
    }

    #[test]
    fn test_rfc5322_domain_literal_with_quoted_pair_brackets() {
        // Escaped brackets in domain literal
        let result = parse_email_address("user@[\\[test\\]]").unwrap();
        assert_eq!(result, "user@[\\[test\\]]");
    }

    #[test]
    fn test_rfc5322_domain_literal_with_backslash() {
        // Escaped backslash in domain literal
        let result = parse_email_address("user@[test\\\\value]").unwrap();
        assert_eq!(result, "user@[test\\\\value]");
    }

    #[test]
    fn test_rfc5322_comments_with_quoted_pairs() {
        // Comments can contain quoted-pairs
        let result = parse_email_address("user(comment\\)with\\)parens)@example.com").unwrap();
        assert_eq!(result, "user@example.com");
    }

    #[test]
    fn test_rfc5322_cfws_complex() {
        // Complex CFWS with multiple comments and FWS
        let result = parse_email_address("user (comment1) \r\n (comment2)@example.com").unwrap();
        assert_eq!(result, "user@example.com");
    }

    #[test]
    fn test_rfc5322_obs_local_part_multiple_words() {
        // Obsolete local-part with multiple words
        let result = parse_email_address("\"user\".\"name\".\"test\"@example.com").unwrap();
        assert_eq!(result, "\"user\".\"name\".\"test\"@example.com");
    }

    #[test]
    fn test_rfc5322_obs_local_part_mixed() {
        // Obsolete local-part mixing quoted strings and atoms
        let result = parse_email_address("\"user\".name.\"test\"@example.com").unwrap();
        assert_eq!(result, "\"user\".name.\"test\"@example.com");
    }

    #[test]
    fn test_rfc5322_obs_domain_multiple_atoms() {
        // Obsolete domain with multiple atoms and comments
        let result = parse_email_address("user@(c1)example(c2).(c3)com(c4)").unwrap();
        assert_eq!(result, "user@example.com");
    }

    #[test]
    fn test_rfc5322_dot_atom_edge_cases() {
        // Valid dot-atom with single character parts
        let result = parse_email_address("a.b.c@example.com").unwrap();
        assert_eq!(result, "a.b.c@example.com");
    }

    #[test]
    fn test_rfc5322_quoted_string_all_quoted_pairs() {
        // Quoted string with only quoted-pairs
        let result = parse_email_address("\"\\\"\\\"\\\"\"@example.com").unwrap();
        assert_eq!(result, "\"\\\"\\\"\\\"\"@example.com");
    }

    #[test]
    fn test_rfc5322_quoted_string_empty_with_fws() {
        // Empty quoted string with FWS
        let result = parse_email_address("\" \"@example.com").unwrap();
        assert_eq!(result, "\" \"@example.com");
    }

    #[test]
    fn test_rfc5322_domain_literal_all_dtext_chars() {
        // Domain literal with various dtext characters
        let result = parse_email_address("user@[!\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ^_`abcdefghijklmnopqrstuvwxyz{|}~]").unwrap();
        assert!(result.contains("@["));
        assert!(result.contains("]"));
    }

    #[test]
    fn test_rfc5322_local_part_max_length_edge() {
        // Test local part at RFC 5321 limit (64 octets)
        let long_local = "a".repeat(64);
        let result = parse_email_address(&format!("{}@example.com", long_local)).unwrap();
        assert_eq!(result, format!("{}@example.com", long_local));
    }

    #[test]
    fn test_rfc5322_domain_max_length_edge() {
        // Test domain at RFC 5321 limit (255 octets)
        // Each label can be up to 63 octets, so we need multiple labels
        let label = "a".repeat(63);
        let domain = format!("{}.{}.{}.{}", label, label, label, "com");
        let result = parse_email_address(&format!("user@{}", domain)).unwrap();
        assert_eq!(result, format!("user@{}", domain));
    }

    #[test]
    fn test_rfc5322_quoted_string_with_all_special_chars_escaped() {
        // Quoted string with all special characters properly escaped
        let result = parse_email_address("\"test\\\"\\@\\!\"@example.com").unwrap();
        assert_eq!(result, "\"test\\\"\\@\\!\"@example.com");
    }

    #[test]
    fn test_rfc5322_nested_comments_deep() {
        // Very deeply nested comments (properly balanced)
        let result = parse_email_address("user((((comment))))@example.com").unwrap();
        assert_eq!(result, "user@example.com");
    }

    #[test]
    fn test_rfc5322_fws_variations() {
        // Various FWS formats
        let result = parse_email_address("user \r\n\t @example.com").unwrap();
        assert_eq!(result, "user@example.com");
    }

    #[test]
    fn test_rfc5322_quoted_string_crlf_only() {
        // CRLF without trailing whitespace - per RFC 5322, FWS requires WSP after CRLF
        // This should fail as it's not valid FWS
        assert!(parse_email_address("\"user\r\nname\"@example.com").is_err());
    }

    #[test]
    fn test_rfc5322_domain_literal_ipv4_with_spaces() {
        // IPv4-like with spaces (valid dtext)
        let result = parse_email_address("user@[192 . 168 . 1 . 1]").unwrap();
        assert_eq!(result, "user@[192 . 168 . 1 . 1]");
    }

    #[test]
    fn test_rfc5322_atext_all_chars_in_local() {
        // All atext characters in local part
        let result = parse_email_address("!#$%&'*+-/=?^_`{|}~@example.com").unwrap();
        assert_eq!(result, "!#$%&'*+-/=?^_`{|}~@example.com");
    }

    #[test]
    fn test_rfc5322_invalid_consecutive_dots() {
        // Consecutive dots should fail
        assert!(parse_email_address("user..name@example.com").is_err());
    }

    #[test]
    fn test_rfc5322_invalid_leading_dot() {
        // Leading dot should fail
        assert!(parse_email_address(".user@example.com").is_err());
    }

    #[test]
    fn test_rfc5322_invalid_trailing_dot() {
        // Trailing dot should fail
        assert!(parse_email_address("user.@example.com").is_err());
    }

    #[test]
    fn test_rfc5322_invalid_unclosed_quote() {
        // Unclosed quoted string should fail
        assert!(parse_email_address("\"user@example.com").is_err());
    }

    #[test]
    fn test_rfc5322_invalid_unclosed_bracket() {
        // Unclosed domain literal should fail
        assert!(parse_email_address("user@[192.168.1.1").is_err());
    }

    #[test]
    fn test_rfc5322_invalid_unclosed_comment() {
        // Unclosed comment should fail
        assert!(parse_email_address("user(comment@example.com").is_err());
    }

    #[test]
    fn test_rfc5322_invalid_quoted_pair_eof() {
        // Quoted-pair at end of string (incomplete)
        assert!(parse_email_address("\"user\\\"@example.com").is_err());
    }

    #[test]
    fn test_rfc5322_valid_quoted_pair_all_vchars() {
        // All valid VCHAR characters in quoted-pair
        for code in 33..=126u32 {
            let c = char::from_u32(code).unwrap();
            let email = format!("\"user\\{}name\"@example.com", c);
            let result = parse_email_address(&email);
            assert!(
                result.is_ok(),
                "Failed to parse quoted-pair with character: {:?} (code {})",
                c,
                code
            );
        }
    }

    // Comprehensive tests for non-RFC compliant addresses

    #[test]
    fn test_invalid_empty_string() {
        assert!(parse_email_address("").is_err());
    }

    #[test]
    fn test_invalid_whitespace_only() {
        // String with only whitespace should fail
        assert!(parse_email_address("   ").is_err());
        assert!(parse_email_address("\t").is_err());
        assert!(parse_email_address("\r\n").is_err());
        assert!(parse_email_address(" \t \r\n ").is_err());
    }

    #[test]
    fn test_invalid_whitespace_only_with_comments() {
        // CFWS only (whitespace/comments) should fail
        assert!(parse_email_address("(comment)").is_err());
        assert!(parse_email_address(" (comment) ").is_err());
        assert!(parse_email_address("(comment1)(comment2)").is_err());
    }

    #[test]
    fn test_invalid_multiple_at_signs() {
        // Multiple @ signs should fail
        assert!(parse_email_address("user@domain@example.com").is_err());
        assert!(parse_email_address("user@@example.com").is_err());
        assert!(parse_email_address("@user@example.com").is_err());
    }

    #[test]
    fn test_invalid_at_sign_in_local_part_unquoted() {
        // @ in local part must be quoted
        assert!(parse_email_address("user@name@example.com").is_err());
    }

    #[test]
    fn test_invalid_at_sign_in_domain() {
        // @ cannot appear in domain (except in quoted strings, but domain doesn't support quoted strings)
        assert!(parse_email_address("user@exam@ple.com").is_err());
    }

    #[test]
    fn test_invalid_space_in_local_part_unquoted() {
        // Space in local part must be quoted
        assert!(parse_email_address("user name@example.com").is_err());
    }

    #[test]
    fn test_invalid_space_in_domain() {
        // Space in domain (outside domain-literal) should fail
        assert!(parse_email_address("user@exam ple.com").is_err());
    }

    #[test]
    fn test_invalid_control_characters() {
        // Control characters (0-31, 127) should fail
        // Note: Tab (9) and space (32) are WSP and valid in CFWS, so skip them
        for code in 0..=31u32 {
            if code != 9 && code != 32 {
                // Skip tab (WSP) and space (WSP)
                let c = char::from_u32(code).unwrap();
                let email = format!("user{}@example.com", c);
                assert!(
                    parse_email_address(&email).is_err(),
                    "Should reject control character: {:?} (code {})",
                    c,
                    code
                );
            }
        }
        // DEL character (127)
        assert!(parse_email_address("user\x7f@example.com").is_err());
    }

    #[test]
    fn test_invalid_quoted_string_with_unescaped_quote() {
        // Unescaped quote in quoted string should fail
        assert!(parse_email_address("\"user\"name\"@example.com").is_err());
    }

    #[test]
    fn test_invalid_quoted_string_with_unescaped_backslash() {
        // Backslash not followed by valid character should fail
        assert!(parse_email_address("\"user\\\"@example.com").is_err());
        // Backslash at end of quoted string
        assert!(parse_email_address("\"user\\\"@example.com").is_err());
    }

    #[test]
    fn test_invalid_quoted_pair_invalid_char() {
        // Quoted-pair with invalid character (control char, DEL)
        for code in 0..=32u32 {
            if code != 9 && code != 32 {
                // Skip tab and space which are valid
                let c = char::from_u32(code).unwrap();
                let email = format!("\"user\\{}name\"@example.com", c);
                assert!(
                    parse_email_address(&email).is_err(),
                    "Should reject quoted-pair with control char: {:?} (code {})",
                    c,
                    code
                );
            }
        }
    }

    #[test]
    fn test_invalid_domain_literal_with_invalid_char() {
        // Invalid characters in domain literal (outside dtext range)
        assert!(parse_email_address("user@[\x00]").is_err()); // NULL
        assert!(parse_email_address("user@[\x1f]").is_err()); // Control char
        assert!(parse_email_address("user@[\x7f]").is_err()); // DEL
    }

    #[test]
    fn test_invalid_domain_literal_unclosed() {
        // Various unclosed domain literal cases
        assert!(parse_email_address("user@[192.168.1.1").is_err());
        assert!(parse_email_address("user@[192.168").is_err());
        assert!(parse_email_address("user@[").is_err());
    }

    #[test]
    fn test_invalid_domain_literal_nested_brackets() {
        // Nested brackets (not allowed, brackets must be escaped)
        assert!(parse_email_address("user@[[test]]").is_err());
    }

    #[test]
    fn test_invalid_comment_unclosed() {
        // Various unclosed comment cases
        assert!(parse_email_address("user(comment@example.com").is_err());
        assert!(parse_email_address("user@example(comment.com").is_err());
        assert!(parse_email_address("(comment@example.com").is_err());
        assert!(parse_email_address("user@example.com(comment").is_err());
    }

    #[test]
    fn test_invalid_comment_mismatched_parens() {
        // Mismatched parentheses
        assert!(parse_email_address("user((comment)@example.com").is_err());
        assert!(parse_email_address("user(comment))@example.com").is_err());
    }

    #[test]
    fn test_invalid_dot_atom_leading_dot() {
        // Leading dot in dot-atom
        assert!(parse_email_address(".user@example.com").is_err());
        assert!(parse_email_address("..user@example.com").is_err());
    }

    #[test]
    fn test_invalid_dot_atom_trailing_dot() {
        // Trailing dot in dot-atom
        assert!(parse_email_address("user.@example.com").is_err());
        assert!(parse_email_address("user..@example.com").is_err());
    }

    #[test]
    fn test_invalid_dot_atom_consecutive_dots() {
        // Consecutive dots in dot-atom
        assert!(parse_email_address("user..name@example.com").is_err());
        assert!(parse_email_address("user...name@example.com").is_err());
        assert!(parse_email_address(".user..name.@example.com").is_err());
    }

    #[test]
    fn test_invalid_domain_leading_dot() {
        // Leading dot in domain
        assert!(parse_email_address("user@.example.com").is_err());
    }

    #[test]
    fn test_invalid_domain_trailing_dot() {
        // Trailing dot in domain
        assert!(parse_email_address("user@example.com.").is_err());
    }

    #[test]
    fn test_invalid_domain_consecutive_dots() {
        // Consecutive dots in domain
        assert!(parse_email_address("user@example..com").is_err());
        assert!(parse_email_address("user@..example.com").is_err());
    }

    #[test]
    fn test_invalid_empty_local_part() {
        // Empty local part (after CFWS removal)
        assert!(parse_email_address("@example.com").is_err());
        assert!(parse_email_address("()@example.com").is_err());
    }

    #[test]
    fn test_invalid_empty_domain() {
        // Empty domain (after CFWS removal)
        assert!(parse_email_address("user@").is_err());
        assert!(parse_email_address("user@()").is_err());
    }

    #[test]
    fn test_invalid_empty_quoted_string() {
        // Empty quoted string is actually valid per RFC 5322, but let's verify it works
        // This should pass - empty quoted string is allowed
        assert!(parse_email_address("\"\"@example.com").is_ok());
    }

    #[test]
    fn test_invalid_empty_domain_literal() {
        // Empty domain literal is actually valid per RFC 5322
        assert!(parse_email_address("user@[]").is_ok());
    }

    #[test]
    fn test_invalid_quoted_string_newline_unquoted() {
        // Newline in quoted string without proper FWS handling
        // CRLF without following WSP is invalid
        assert!(parse_email_address("\"user\r\nname\"@example.com").is_err());
    }

    #[test]
    fn test_invalid_fws_crlf_without_wsp() {
        // CRLF without following whitespace is invalid FWS
        assert!(parse_email_address("user\r\n@example.com").is_err());
    }

    #[test]
    fn test_invalid_quoted_string_with_ctext_control_char() {
        // Control characters in quoted string (outside quoted-pair) should fail
        for code in 0..=31u32 {
            if code != 9 {
                // Tab is WSP, allowed
                let c = char::from_u32(code).unwrap();
                let email = format!("\"user{}name\"@example.com", c);
                assert!(
                    parse_email_address(&email).is_err(),
                    "Should reject control char in quoted string: {:?} (code {})",
                    c,
                    code
                );
            }
        }
    }

    #[test]
    fn test_invalid_comment_with_control_char() {
        // Control characters in comment (outside quoted-pair) should fail
        for code in 0..=31u32 {
            if code != 9 {
                // Tab is WSP, allowed
                let c = char::from_u32(code).unwrap();
                let email = format!("user({}comment)@example.com", c);
                assert!(
                    parse_email_address(&email).is_err(),
                    "Should reject control char in comment: {:?} (code {})",
                    c,
                    code
                );
            }
        }
    }

    #[test]
    fn test_invalid_obs_local_part_empty_word() {
        // Obsolete local-part with empty word should fail
        assert!(parse_email_address("\"user\"..\"name\"@example.com").is_err());
        assert!(parse_email_address(".\"user\"@example.com").is_err());
        assert!(parse_email_address("\"user\".@example.com").is_err());
    }

    #[test]
    fn test_invalid_obs_domain_empty_atom() {
        // Obsolete domain with empty atom should fail
        assert!(parse_email_address("user@.example.com").is_err());
        assert!(parse_email_address("user@example..com").is_err());
        assert!(parse_email_address("user@example.com.").is_err());
    }

    #[test]
    fn test_invalid_unicode_characters() {
        // Non-ASCII characters should fail (RFC 5322 is ASCII-only)
        assert!(parse_email_address("usér@example.com").is_err());
        assert!(parse_email_address("user@exämple.com").is_err());
        assert!(parse_email_address("user@example.cöm").is_err());
    }

    #[test]
    fn test_invalid_quoted_string_unicode() {
        // Unicode in quoted string should fail
        assert!(parse_email_address("\"usér\"@example.com").is_err());
    }

    #[test]
    fn test_invalid_domain_literal_unicode() {
        // Unicode in domain literal should fail
        assert!(parse_email_address("user@[exämple]").is_err());
    }

    #[test]
    fn test_invalid_mixed_case_special_chars() {
        // Various invalid combinations
        assert!(parse_email_address("user@exam@ple.com").is_err());
        assert!(parse_email_address("user name@example.com").is_err());
        assert!(parse_email_address("user@exam ple.com").is_err());
    }

    #[test]
    fn test_invalid_quoted_string_malformed() {
        // Malformed quoted strings
        assert!(parse_email_address("\"user\"\"name\"@example.com").is_err());
        assert!(parse_email_address("\"user\"name\"@example.com").is_err());
    }

    #[test]
    fn test_invalid_domain_literal_malformed() {
        // Malformed domain literals
        assert!(parse_email_address("user@[]test]").is_err());
        assert!(parse_email_address("user@[test[]").is_err());
    }

    #[test]
    fn test_whitespace_around_at_rfc_compliance() {
        // Whitespace around @ sign per RFC 5322
        // According to RFC 5322:
        // - dot-atom = [CFWS] dot-atom-text [CFWS]
        // - So CFWS can appear before/after dot-atom-text in both local-part and domain
        // - "user@ example.com" is valid: space is leading CFWS of domain's dot-atom
        // - "user @example.com" is valid: space is trailing CFWS of local-part's dot-atom
        // - "user @ example.com" is valid: both trailing CFWS of local-part and leading CFWS of domain
        // All of these are RFC 5322 compliant
        assert!(parse_email_address("user@ example.com").is_ok());
        assert!(parse_email_address("user @example.com").is_ok());
        assert!(parse_email_address("user @ example.com").is_ok());
    }

    #[test]
    fn test_invalid_quoted_pair_backslash_only() {
        // Backslash at end (incomplete quoted-pair)
        assert!(parse_email_address("\"user\\\"@example.com").is_err());
        assert!(parse_email_address("user\\@example.com").is_err());
    }

    #[test]
    fn test_invalid_domain_literal_quoted_pair_backslash_only() {
        // Backslash at end in domain literal
        assert!(parse_email_address("user@[test\\").is_err());
    }

    #[test]
    fn test_invalid_trailing_garbage() {
        // Parser should reject input with trailing garbage after valid addr-spec
        assert!(parse_email_address("user@example.com garbage").is_err());
        assert!(parse_email_address("user@example.com@garbage.com").is_err());
        assert!(parse_email_address("user@example.com ").is_ok()); // Trailing CFWS is valid
        assert!(parse_email_address("user@example.com(comment)").is_ok()); // Trailing CFWS is valid
    }
}
