//! Bot commands: parsing of typed commands and the Telegram command menu.
//!
//! French users get French names (`/liste`, `/aide`), everyone else English ones (`/list`,
//! `/help`); both are accepted in every language, as well as the older `/start` and `/recipes`.

use anyhow::Result;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::BotCommand;

use crate::localization::{detect_language, t_lang, LocalizationManager};

/// A command typed by the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `/start`: welcome message.
    Start,
    /// `/aide`, `/help`: help message.
    Help,
    /// `/liste`, `/list`, `/recipes`: first page of the recipe list.
    List,
    /// `/cherche`, `/search`, with the words that follow (possibly empty).
    Search(String),
}

/// Reads a message text as a command, or returns `None` when it is not a known command.
///
/// The name is case-insensitive (phone keyboards capitalize the first letter) and may carry the
/// bot username that Telegram adds in groups (`/liste@JustIngredientsBot`).
pub fn parse_command(text: &str) -> Option<Command> {
    let rest = text.strip_prefix('/')?;
    let (head, args) = rest
        .split_once(char::is_whitespace)
        .map_or((rest, ""), |(head, args)| (head, args.trim()));
    let name = head
        .split_once('@')
        .map_or(head, |(name, _)| name)
        .to_lowercase();
    match name.as_str() {
        "start" => Some(Command::Start),
        "aide" | "help" => Some(Command::Help),
        "liste" | "list" | "recipes" => Some(Command::List),
        "cherche" | "search" => Some(Command::Search(args.to_string())),
        _ => None,
    }
}

/// What a typed text is, given whether an action (a dialogue) is running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextRoute {
    /// Starts with `/`: a command (`None` when unknown). It always wins, even during an action:
    /// it is never taken as a name, a quantity or an ingredient.
    Command(Option<Command>),
    /// An answer to the running action.
    ActionInput,
    /// Text outside any action.
    FreeText,
}

/// Routes a typed text (pure, see [`TextRoute`]).
pub fn route_text(text: &str, in_action: bool) -> TextRoute {
    if text.starts_with('/') {
        TextRoute::Command(parse_command(text))
    } else if in_action {
        TextRoute::ActionInput
    } else {
        TextRoute::FreeText
    }
}

/// Commands of the menu for a supported language: command name and description key.
///
/// `/cherche` joins the menu once search works (UX plan, Task U4.2).
fn menu_entries(language: &str) -> &'static [(&'static str, &'static str)] {
    match language {
        "fr" => &[("liste", "command-list"), ("aide", "command-help")],
        _ => &[("list", "command-list"), ("help", "command-help")],
    }
}

/// Telegram menu commands for a supported language (`"en"` or `"fr"`).
pub fn menu_commands(localization: &Arc<LocalizationManager>, language: &str) -> Vec<BotCommand> {
    menu_entries(language)
        .iter()
        .map(|(command, key)| BotCommand::new(*command, t_lang(localization, key, Some(language))))
        .collect()
}

/// One "/command - description" line per menu command, in the user's language, for the welcome
/// and help messages.
pub fn command_lines(
    localization: &Arc<LocalizationManager>,
    language_code: Option<&str>,
) -> String {
    let language = detect_language(localization, language_code);
    menu_commands(localization, &language)
        .iter()
        .map(|c| format!("/{} - {}", c.command, c.description))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Sets the menu shown by the Telegram app: French for French-speaking users, English for
/// everyone else.
pub async fn set_command_menu(bot: &Bot, localization: &Arc<LocalizationManager>) -> Result<()> {
    bot.set_my_commands(menu_commands(localization, "en"))
        .await?;
    bot.set_my_commands(menu_commands(localization, "fr"))
        .language_code("fr")
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::localization::create_localization_manager;

    #[test]
    fn parses_french_english_and_legacy_names() {
        assert_eq!(parse_command("/start"), Some(Command::Start));
        assert_eq!(parse_command("/aide"), Some(Command::Help));
        assert_eq!(parse_command("/help"), Some(Command::Help));
        assert_eq!(parse_command("/liste"), Some(Command::List));
        assert_eq!(parse_command("/list"), Some(Command::List));
        assert_eq!(parse_command("/recipes"), Some(Command::List));
        assert_eq!(
            parse_command("/cherche"),
            Some(Command::Search(String::new()))
        );
        assert_eq!(
            parse_command("/search"),
            Some(Command::Search(String::new()))
        );
    }

    #[test]
    fn ignores_case_and_bot_username() {
        assert_eq!(parse_command("/Liste"), Some(Command::List));
        assert_eq!(parse_command("/AIDE"), Some(Command::Help));
        assert_eq!(
            parse_command("/liste@JustIngredientsBot"),
            Some(Command::List)
        );
        assert_eq!(
            parse_command("/cherche@JustIngredientsBot farine"),
            Some(Command::Search("farine".to_string()))
        );
    }

    #[test]
    fn keeps_search_words_and_ignores_other_arguments() {
        assert_eq!(
            parse_command("/cherche  crème brûlée "),
            Some(Command::Search("crème brûlée".to_string()))
        );
        assert_eq!(parse_command("/start deep-link"), Some(Command::Start));
        assert_eq!(parse_command("/liste 2"), Some(Command::List));
    }

    #[test]
    fn rejects_unknown_commands_and_plain_text() {
        assert_eq!(parse_command("/recipesfoo"), None);
        assert_eq!(parse_command("/listes"), None);
        assert_eq!(parse_command("/"), None);
        assert_eq!(parse_command("liste"), None);
        assert_eq!(parse_command("2 cups flour"), None);
        assert_eq!(parse_command(" /liste"), None);
    }

    #[test]
    fn commands_win_over_the_running_action() {
        assert_eq!(
            route_text("/aide", true),
            TextRoute::Command(Some(Command::Help))
        );
        assert_eq!(
            route_text("/liste", true),
            TextRoute::Command(Some(Command::List))
        );
        assert_eq!(route_text("/inconnue", true), TextRoute::Command(None));
        assert_eq!(route_text("Tarte aux pommes", true), TextRoute::ActionInput);
        assert_eq!(route_text("200 g farine", true), TextRoute::ActionInput);
    }

    #[test]
    fn text_outside_an_action_is_free_text_unless_it_is_a_command() {
        assert_eq!(
            route_text("/start", false),
            TextRoute::Command(Some(Command::Start))
        );
        assert_eq!(route_text("/foo", false), TextRoute::Command(None));
        assert_eq!(route_text("bonjour", false), TextRoute::FreeText);
    }

    #[test]
    fn menu_follows_telegram_rules_and_parses_back() {
        let localization = create_localization_manager().expect("localization");
        for language in ["en", "fr"] {
            let commands = menu_commands(&localization, language);
            assert!(!commands.is_empty());
            for c in &commands {
                assert!(
                    (1..=32).contains(&c.command.len())
                        && c.command
                            .chars()
                            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_'),
                    "{language}: invalid command name {:?}",
                    c.command
                );
                assert!(
                    (3..=256).contains(&c.description.chars().count())
                        && !c.description.starts_with("Missing translation"),
                    "{language}: invalid description for /{}: {:?}",
                    c.command,
                    c.description
                );
                assert!(
                    parse_command(&format!("/{}", c.command)).is_some(),
                    "{language}: /{} is in the menu but not understood",
                    c.command
                );
            }
        }
    }

    #[test]
    fn command_lines_use_the_user_language() {
        let localization = create_localization_manager().expect("localization");
        let fr = command_lines(&localization, Some("fr-FR"));
        assert!(fr.contains("/liste - ") && fr.contains("/aide - "), "{fr}");
        let en = command_lines(&localization, Some("de"));
        assert!(en.contains("/list - ") && en.contains("/help - "), "{en}");
    }
}
