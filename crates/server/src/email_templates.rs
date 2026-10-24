//! Plain, branded HTML + text email templates. No external images; inline styles only.

/// A rendered message: subject plus text and HTML bodies.
#[derive(Debug, Clone)]
pub struct Rendered {
    pub subject: String,
    pub text: String,
    pub html: String,
}

pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

struct Parts<'a> {
    preheader: &'a str,
    heading: &'a str,
    /// Paragraphs, plain text (escaped for the HTML version).
    paragraphs: Vec<String>,
    button: Option<(&'a str, &'a str)>,
    footer: &'a str,
}

fn render(subject: &str, p: Parts) -> Rendered {
    let mut text = format!("{}\n\n", p.heading);
    for para in &p.paragraphs {
        text.push_str(para);
        text.push_str("\n\n");
    }
    if let Some((label, url)) = p.button {
        text.push_str(&format!("{label}: {url}\n\n"));
    }
    text.push_str(&format!("-- \nWartide\n{}\n", p.footer));

    let mut body = String::new();
    for para in &p.paragraphs {
        body.push_str(&format!(
            "<p style=\"margin:0 0 16px;font-size:16px;line-height:1.5;color:#2b2f36\">{}</p>",
            escape_html(para).replace('\n', "<br>")
        ));
    }
    let button = p
        .button
        .map(|(label, url)| {
            format!(
                "<p style=\"margin:24px 0\"><a href=\"{u}\" style=\"background:#9c3b24;color:#ffffff;text-decoration:none;padding:12px 22px;border-radius:6px;font-weight:600;display:inline-block\">{l}</a></p>\
                 <p style=\"margin:0 0 16px;font-size:13px;color:#6b7280;word-break:break-all\">Or open this link: <a href=\"{u}\" style=\"color:#9c3b24\">{u}</a></p>",
                u = escape_html(url),
                l = escape_html(label)
            )
        })
        .unwrap_or_default();
    let html = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>{title}</title></head>\
         <body style=\"margin:0;padding:0;background:#f3f1ec;font-family:-apple-system,Segoe UI,Helvetica,Arial,sans-serif\">\
         <span style=\"display:none;max-height:0;overflow:hidden;opacity:0\">{pre}</span>\
         <table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" style=\"background:#f3f1ec\"><tr><td align=\"center\" style=\"padding:24px 12px\">\
         <table role=\"presentation\" width=\"560\" cellpadding=\"0\" cellspacing=\"0\" style=\"max-width:560px;width:100%;background:#ffffff;border-radius:8px\">\
         <tr><td style=\"background:#23282f;color:#f3f1ec;padding:18px 28px;border-radius:8px 8px 0 0;font-size:22px;font-weight:700;letter-spacing:2px\">WARTIDE</td></tr>\
         <tr><td style=\"padding:28px\"><h1 style=\"margin:0 0 18px;font-size:22px;color:#23282f\">{h}</h1>{body}{button}</td></tr>\
         <tr><td style=\"padding:16px 28px 24px;font-size:12px;color:#6b7280;border-top:1px solid #e5e1d8\">{footer}</td></tr>\
         </table></td></tr></table></body></html>",
        title = escape_html(subject),
        pre = escape_html(p.preheader),
        h = escape_html(p.heading),
        footer = escape_html(p.footer),
    );
    Rendered {
        subject: subject.to_string(),
        text,
        html,
    }
}

pub fn verification(username: &str, link: &str) -> Rendered {
    render(
        "Verify your Wartide email",
        Parts {
            preheader: "Confirm your email address to start playing.",
            heading: "Welcome to Wartide",
            paragraphs: vec![
                format!("Hi {username}, confirm your email address to create and join games."),
                "The link expires in 48 hours.".into(),
            ],
            button: Some(("Verify email", link)),
            footer: "You received this because an account was registered with this address.",
        },
    )
}

pub fn password_reset(username: &str, link: &str) -> Rendered {
    render(
        "Reset your Wartide password",
        Parts {
            preheader: "Use this link to choose a new password.",
            heading: "Reset your password",
            paragraphs: vec![
                format!("Hi {username}, we received a request to reset your password."),
                "The link expires in 2 hours. If you did not ask for this, you can ignore this email."
                    .into(),
            ],
            button: Some(("Choose a new password", link)),
            footer: "For your security, this link works once.",
        },
    )
}

pub fn your_turn(
    username: &str,
    game_name: &str,
    power: &str,
    round: i32,
    link: &str,
    settings_link: &str,
) -> Rendered {
    let footer = format!("Turn it off any time in your settings: {settings_link}");
    render(
        "It's your turn in Wartide",
        Parts {
            preheader: "A game is waiting for your orders.",
            heading: "It's your turn",
            paragraphs: vec![format!(
                "Hi {username}, {game_name} is waiting for you: {power}, round {round}."
            )],
            button: Some(("Open game", link)),
            footer: &footer,
        },
    )
}

pub fn match_found(
    username: &str,
    game_name: &str,
    mode: &str,
    link: &str,
    settings_link: &str,
) -> Rendered {
    let footer = format!("Turn it off any time in your settings: {settings_link}");
    render(
        "Your Wartide match is ready",
        Parts {
            preheader: "An opponent was found. The game has started.",
            heading: "Match found",
            paragraphs: vec![format!(
                "Hi {username}, your {mode} match ({game_name}) has started."
            )],
            button: Some(("Open game", link)),
            footer: &footer,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_contain_link_and_brand_and_escape() {
        let m = verification("<b>bob</b>", "http://x/verify?token=abc");
        assert!(m.text.contains("http://x/verify?token=abc"));
        assert!(m.text.contains("Wartide"));
        assert!(m.html.contains("WARTIDE"));
        assert!(m.html.contains("&lt;b&gt;bob&lt;/b&gt;"));
        assert!(!m.html.contains("<b>bob"));
        assert!(!m.html.contains("<img"));
    }

    #[test]
    fn all_templates_render() {
        for m in [
            password_reset("a", "http://x/reset-password?token=t"),
            your_turn(
                "a",
                "Game",
                "Germany",
                3,
                "http://x/games/1",
                "http://x/settings",
            ),
            match_found(
                "a",
                "Ranked 2p #1000",
                "2-player",
                "http://x/games/1",
                "http://x/settings",
            ),
        ] {
            assert!(
                !m.subject.is_empty() && m.text.contains("http://x/") && m.html.contains("<html>")
            );
        }
    }
}
