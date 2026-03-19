# Telegram Bot Feature Specification

## Overview
The Telegram bot feature provides the user interface for the ingredient extraction service. It handles incoming messages, processes images, manages conversations, and delivers formatted results to users.

## Requirements

### Functional Requirements
- **Message Handling**: Process text, photos, documents, and commands
- **Image Processing**: Accept and validate image uploads
- **Conversation Management**: Multi-step recipe creation workflow
- **Command Support**: /start, /help, and other bot commands
- **Localization**: Support for English and French languages
- **Error Recovery**: Graceful handling of processing failures
- **Result Formatting**: Clear presentation of extracted ingredients

### Non-Functional Requirements
- **Responsiveness**: Quick acknowledgment of user inputs
- **Reliability**: Handle network issues and API failures
- **User Experience**: Intuitive conversation flow
- **Scalability**: Support multiple concurrent users
- **Security**: Safe file handling and input validation

### Success Criteria
- [x] All message types handled appropriately
- [x] Image upload and processing works seamlessly
- [x] Conversation state maintained correctly
- [x] Commands respond with helpful information
- [x] Multi-language support functional
- [x] Error messages are user-friendly
- [x] Inline ingredient editing supported
- [x] Interactive callback buttons for review and editing

## Architecture

### Component Design
```
Telegram Bot Layer (src/bot/)
├── message_handler.rs      # Route incoming messages
├── media_handlers.rs       # Handle photo/document uploads
├── dialogue_manager.rs     # Conversation state and flow
├── command_handlers.rs     # Bot commands (/start, /help, /list, /search, /delete)
├── ui_builder.rs          # Response formatting and keyboards
├── ui_components.rs       # Reusable UI component helpers
├── image_processing.rs    # Image download and temp file management
├── localization.rs        # Multi-language support (Fluent bundles)
└── callbacks/
    ├── callback_handler.rs    # Dispatch incoming callback queries
    ├── callback_types.rs      # Typed callback action enums
    ├── editing_callbacks.rs   # Ingredient edit/delete/add callbacks
    ├── recipe_callbacks.rs    # Recipe save/rename/delete callbacks
    ├── review_callbacks.rs    # Review confirmation and adjustment callbacks
    └── workflow_callbacks.rs  # High-level workflow transitions
```

### Data Flow
1. **Message Reception**: Telegram webhook delivers updates
2. **Message Routing**: Determine message type and handler
3. **Validation**: Check input format and permissions
4. **Processing**: Execute appropriate business logic
5. **Response Formatting**: Create user-friendly output
6. **Message Delivery**: Send response via Telegram API

### Integration Points
- **OCR Processing**: Extract text from uploaded images
- **Database**: Store user data and conversation history
- **Localization**: Provide translated responses
- **Error Handling**: Centralized error response formatting

## API Design

### Message Handlers
```rust
// Main message processing
pub async fn handle_message(bot: &Bot, message: &Message, dialogue: &mut Dialogue) -> Result<(), BotError>

// Photo processing
pub async fn handle_photo(bot: &Bot, message: &Message, dialogue: &Dialogue) -> Result<(), BotError>

// Document processing
pub async fn handle_document(bot: &Bot, message: &Message, dialogue: &Dialogue) -> Result<(), BotError>

// Command handling
pub async fn handle_command(bot: &Bot, message: &Message, command: &str) -> Result<(), BotError>
```

### Dialogue States
```rust
/// Conversation state machine for recipe creation workflow
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub enum RecipeDialogueState {
    #[default]
    Start,
    WaitingForRecipeName {
        extracted_text: String,
        ingredients: Vec<MeasurementMatch>,
        language_code: Option<String>,
    },
    ReviewIngredients {
        recipe_name: String,
        ingredients: Vec<MeasurementMatch>,
        language_code: Option<String>,
        message_id: Option<i32>,
        extracted_text: String,
        recipe_name_from_caption: Option<String>,
    },
    EditingIngredient {
        recipe_name: String,
        ingredients: Vec<MeasurementMatch>,
        editing_index: usize,
        language_code: Option<String>,
        message_id: Option<i32>,
        original_message_id: Option<i32>,
        extracted_text: String,
        recipe_name_from_caption: Option<String>,
    },
    WaitingForRecipeNameAfterConfirm {
        ingredients: Vec<MeasurementMatch>,
        language_code: Option<String>,
        extracted_text: String,
        recipe_name_from_caption: Option<String>,
        message_id: Option<i32>,
    },
    RenamingRecipe {
        recipe_id: i64,
        current_name: String,
        language_code: Option<String>,
    },
    EditingSavedIngredients {
        recipe_id: i64,
        original_ingredients: Vec<Ingredient>,
        current_matches: Vec<MeasurementMatch>,
        language_code: Option<String>,
        message_id: Option<i32>,
    },
    EditingSavedIngredient {
        recipe_id: i64,
        original_ingredients: Vec<Ingredient>,
        current_matches: Vec<MeasurementMatch>,
        editing_index: usize,
        language_code: Option<String>,
        message_id: Option<i32>,
        original_message_id: Option<i32>,
    },
    AddingIngredientToSavedRecipe {
        recipe_id: i64,
        original_ingredients: Vec<Ingredient>,
        current_matches: Vec<MeasurementMatch>,
        language_code: Option<String>,
        message_id: Option<i32>,
    },
    AwaitingQuantityCorrection {
        recipe_name: String,
        ingredients: Vec<MeasurementMatch>,
        ingredient_index: usize,
        language_code: Option<String>,
        message_id: Option<i32>,
        extracted_text: String,
        recipe_name_from_caption: Option<String>,
    },
}
```

### Error Handling
- **NetworkError**: Telegram API communication failures
- **ValidationError**: Invalid user input or file format
- **ProcessingError**: OCR or database operation failures
- **TimeoutError**: Operations exceeding time limits

## Implementation Notes

### Key Decisions
- **Dialogue Framework**: Teloxide dialogue system for state management
- **Webhook Integration**: Real-time message processing
- **File Download**: Secure temporary file handling
- **Keyboard UI**: Interactive buttons for user choices
- **Localization**: Fluent bundles for i18n support

### Constraints
- **Rate Limits**: Respect Telegram API limitations
- **File Size Limits**: Telegram's 20MB file size restriction
- **Timeout Handling**: 30-second processing windows
- **Memory Management**: Clean up temporary files immediately

### Dependencies
- **teloxide**: Telegram bot framework
- **tokio**: Async runtime for concurrent processing
- **fluent-bundle**: Internationalization support
- **tempfile**: Secure temporary file creation

## User Experience Flow

### Recipe Creation Workflow
1. **Start**: User sends /start or sends photo directly
2. **Image Upload**: User sends photo or document (with optional caption as recipe name)
3. **Processing**: Bot shows progress, calls OCR service
4. **Review**: Display extracted ingredients with confidence; user can accept, edit, or delete each
5. **Recipe Naming**: Prompt for (or confirm caption as) recipe name
6. **Save**: Persist recipe and ingredients to database

### Recipe Management Workflow
- `/list` — paginated list of saved recipes with inline action buttons
- `/search <query>` — full-text search across recipe names and ingredients
- `/delete` — select and delete a saved recipe
- Inline edit: rename recipe, edit/delete/add individual ingredients from saved recipes

### Error Recovery
- **Invalid Input**: Clear error messages with guidance
- **Processing Failure**: Retry options and alternative suggestions
- **Timeout**: Progress indication and retry mechanisms
- **Network Issues**: Automatic retry with exponential backoff

## Testing Strategy

### Unit Tests
- Message parsing and routing logic
- Dialogue state transitions
- Input validation functions
- Response formatting

### Integration Tests
- End-to-end conversation flows
- Image upload and processing
- Database interaction during conversations
- Localization functionality

### User Acceptance Tests
- Real Telegram bot interaction
- Error scenario handling
- Multi-language support validation

## Monitoring & Observability

### Metrics
- Message processing rates
- Conversation completion rates
- Error rates by type
- User engagement metrics

### Logging
- Message receipt and processing
- State transitions
- Error conditions with context
- Performance timing

## Future Enhancements

### Potential Improvements
- **Bulk Operations**: Process multiple images at once
- **User Preferences**: Per-user language and notification settings
- **Sharing Features**: Export recipes (e.g., PDF, CSV)
- **Analytics**: Usage statistics and ingredient insights