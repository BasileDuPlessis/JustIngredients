//! Messages in Telegram HTML (`parse_mode = HTML`), safe by construction.
//!
//! An [`Html`] value is only built from escaped text and the tags of this module (`<b>`,
//! `<pre>`), so a recipe name, an ingredient or a caption containing `<`, `>` or `&` can never
//! break the markup.
//! [`send_html`] and [`edit_html`] are the only way to send one: `Html` does not convert to
//! `String`, so it cannot be sent as plain text by mistake, and plain text cannot be sent as HTML.

use std::ops::{Add, AddAssign};
use teloxide::prelude::*;
use teloxide::types::{MessageId, ParseMode};

/// Telegram HTML text whose user and model parts are escaped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Html(String);

impl Html {
    /// Plain text, escaped.
    pub fn text(plain: &str) -> Self {
        Html(escape(plain))
    }

    /// Plain text in bold, escaped.
    pub fn bold(plain: &str) -> Self {
        Html(format!("<b>{}</b>", escape(plain)))
    }

    /// Plain text as a preformatted block (monospace, kept as is), escaped.
    pub fn pre(plain: &str) -> Self {
        Html(format!("<pre>{}</pre>", escape(plain)))
    }

    /// The markup sent to Telegram.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Add for Html {
    type Output = Html;

    fn add(mut self, other: Html) -> Html {
        self.0.push_str(&other.0);
        self
    }
}

impl AddAssign for Html {
    fn add_assign(&mut self, other: Html) {
        self.0.push_str(&other.0);
    }
}

/// Escapes the three characters Telegram HTML reserves.
fn escape(plain: &str) -> String {
    let mut escaped = String::with_capacity(plain.len());
    for c in plain.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// Sends an HTML message; chain `.reply_markup(...)` as with `send_message`.
pub fn send_html(bot: &Bot, chat: ChatId, html: Html) -> <Bot as Requester>::SendMessage {
    bot.send_message(chat, html.0).parse_mode(ParseMode::Html)
}

/// Replaces the text of a message with HTML; chain `.reply_markup(...)` as with
/// `edit_message_text`.
pub fn edit_html(
    bot: &Bot,
    chat: ChatId,
    message: MessageId,
    html: Html,
) -> <Bot as Requester>::EditMessageText {
    bot.edit_message_text(chat, message, html.0)
        .parse_mode(ParseMode::Html)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_escapes_the_reserved_characters() {
        assert_eq!(
            Html::text("<script>alert('x')</script>").as_str(),
            "&lt;script&gt;alert('x')&lt;/script&gt;"
        );
        assert_eq!(Html::text("Fish & Chips").as_str(), "Fish &amp; Chips");
        assert_eq!(Html::text("&amp;").as_str(), "&amp;amp;");
    }

    #[test]
    fn markdown_and_quotes_stay_as_they_are() {
        assert_eq!(
            Html::text("*Tarte* de \"Mamie\" à 1/2 _").as_str(),
            "*Tarte* de \"Mamie\" à 1/2 _"
        );
    }

    #[test]
    fn bold_wraps_the_escaped_text() {
        assert_eq!(
            Html::bold("Crème <brûlée>").as_str(),
            "<b>Crème &lt;brûlée&gt;</b>"
        );
    }

    #[test]
    fn pre_keeps_the_lines_and_escapes_them() {
        assert_eq!(
            Html::pre("200 g <farine>\n1 œuf").as_str(),
            "<pre>200 g &lt;farine&gt;\n1 œuf</pre>"
        );
    }

    #[test]
    fn parts_are_joined_in_order() {
        let mut html = Html::text("📝 ") + Html::bold("Titre") + Html::text("\n\n");
        html += Html::text("2 < 3");
        assert_eq!(html.as_str(), "📝 <b>Titre</b>\n\n2 &lt; 3");
    }
}
