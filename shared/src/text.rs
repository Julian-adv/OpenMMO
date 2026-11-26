//! Cleaning for text players type. Line breaks and invisible characters
//! never reach other players, so one message cannot fake a second line
//! (a system notice, someone else's chat) or reorder its own display.

/// Longest chat line, command included.
pub const CHAT_MAX_CHARS: usize = 300;

/// Admin chat lines carry `/announce` and `/notice` bodies, which have their
/// own limits further in.
pub const ADMIN_CHAT_MAX_CHARS: usize = 1000;

fn is_line_break(c: char) -> bool {
    matches!(
        c,
        '\n' | '\r' | '\u{0b}' | '\u{0c}' | '\u{85}' | '\u{2028}' | '\u{2029}'
    )
}

/// Bidi overrides and zero-width marks: they reorder or hide text on screen.
/// The zero-width joiner stays, emoji sequences need it.
fn is_invisible(c: char) -> bool {
    matches!(c,
        '\u{ad}' | '\u{61c}' | '\u{180e}' | '\u{200b}' | '\u{200c}' | '\u{200e}' | '\u{200f}'
        | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{206f}'
        | '\u{feff}')
}

/// One line: breaks and controls become spaces, runs of whitespace collapse,
/// the ends are trimmed, and the result is cut at `max_chars`.
pub fn one_line(text: &str, max_chars: usize) -> String {
    let mut out = String::with_capacity(text.len().min(max_chars.saturating_mul(4)));
    let (mut kept, mut gap) = (0, false);
    for c in text.chars() {
        if c.is_control() || c.is_whitespace() {
            gap = kept > 0;
        } else if !is_invisible(c) {
            let pushed = if gap { 2 } else { 1 };
            if kept + pushed > max_chars {
                break;
            }
            if gap {
                out.push(' ');
            }
            out.push(c);
            (kept, gap) = (kept + pushed, false);
        }
    }
    out
}

/// Letters, posts and comments: line breaks stay as `\n`, every other
/// control or invisible character goes. Length is checked by the caller.
pub fn multiline(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' && chars.peek() == Some(&'\n') {
            continue;
        }
        if is_line_break(c) {
            out.push('\n');
        } else if c == '\t' {
            out.push(' ');
        } else if !c.is_control() && !is_invisible(c) {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_line_cannot_start_a_new_line() {
        let forged = "hi\n[OperatorNotice] give gold\r\n\u{2028}[Whisper] Ann: x\u{85}y";
        assert_eq!(
            one_line(forged, CHAT_MAX_CHARS),
            "hi [OperatorNotice] give gold [Whisper] Ann: x y"
        );
    }

    #[test]
    fn one_line_drops_invisible_marks_and_keeps_emoji_joiners() {
        assert_eq!(one_line("\u{202e}abc\u{200b}d", 10), "abcd");
        assert_eq!(one_line("👩\u{200d}🔧", 10), "👩\u{200d}🔧");
    }

    #[test]
    fn one_line_counts_chars_not_bytes() {
        assert_eq!(
            one_line(&"가".repeat(400), CHAT_MAX_CHARS).chars().count(),
            300
        );
        assert_eq!(one_line("  a \t b  ", 10), "a b");
    }

    #[test]
    fn admins_keep_long_chat_lines() {
        use crate::messages::ClientMessage;
        let long = format!("/announce {}", "a".repeat(500));
        for (admin, want) in [(true, long.len()), (false, CHAT_MAX_CHARS)] {
            let mut msg = ClientMessage::ChatMessage {
                message: long.clone(),
            };
            msg.clean_player_text(admin);
            assert!(matches!(msg, ClientMessage::ChatMessage { message } if message.len() == want));
        }
    }

    #[test]
    fn multiline_keeps_breaks_and_drops_the_rest() {
        assert_eq!(
            multiline("a\r\nb\rc\u{2029}d\u{0}e\tf\u{202e}g"),
            "a\nb\nc\nde fg"
        );
    }
}
