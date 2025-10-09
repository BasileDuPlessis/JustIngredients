# Task 2 Implementation: Data Structure Updates

## Summary
Successfully updated data structures to support photo captions as recipe name candidates.

## Changes Made

### 2.1 ✅ ImageProcessingParams Struct Updated
**File**: `src/bot/message_handler.rs:43-50`

**Before**:
```rust
#[derive(Debug)]
pub struct ImageProcessingParams<'a> {
    pub file_id: teloxide::types::FileId,
    pub chat_id: ChatId,
    pub success_message: &'a str,
    pub language_code: Option<&'a str>,
    pub dialogue: RecipeDialogue,
    pub pool: Arc<PgPool>,
}
```

**After**:
```rust
#[derive(Debug)]
pub struct ImageProcessingParams<'a> {
    pub file_id: teloxide::types::FileId,
    pub chat_id: ChatId,
    pub success_message: &'a str,
    pub language_code: Option<&'a str>,
    pub dialogue: RecipeDialogue,
    pub pool: Arc<PgPool>,
    pub caption: Option<String>,  // NEW: Caption from photo message
}
```

### 2.2 ✅ All Instantiations Updated

#### Photo Handler (`handle_photo_message`)
**Location**: `src/bot/message_handler.rs:475-495`

**Added caption extraction**:
```rust
// Extract caption if present - this will be used as recipe name candidate
let caption = msg.caption().map(|s| s.to_string());
```

**Updated ImageProcessingParams**:
```rust
ImageProcessingParams {
    file_id: largest_photo.file.id.clone(),
    chat_id: msg.chat.id,
    success_message: &t_lang(localization, "processing-photo", language_code),
    language_code,
    dialogue,
    pool,
    caption,  // NEW: Pass extracted caption
}
```

#### Document Handler (`handle_document_message`)
**Location**: `src/bot/message_handler.rs:515-530`

**Updated ImageProcessingParams**:
```rust
ImageProcessingParams {
    file_id: doc.file.id.clone(),
    chat_id: msg.chat.id,
    success_message: &t_lang(localization, "processing-document", language_code),
    language_code,
    dialogue,
    pool,
    caption: None,  // Documents don't have captions like photos do
}
```

### 2.3 ✅ Parameter Destructuring Updated
**Location**: `src/bot/message_handler.rs:89-97`

**Updated destructuring in `download_and_process_image`**:
```rust
let ImageProcessingParams {
    file_id,
    chat_id,
    success_message,
    language_code,
    dialogue,
    pool: _pool,
    caption,  // NEW: Extract caption parameter
} = params;
```

## Technical Details

### Caption Extraction Logic
- **Photos**: `msg.caption().map(|s| s.to_string())` - Extracts caption if present
- **Documents**: `None` - Documents don't support captions in Telegram API
- **Type**: `Option<String>` to handle both present and absent captions

### Backward Compatibility
- ✅ Photos without captions: `caption = None` → fallback to "Recipe"
- ✅ Existing document handling: unchanged behavior
- ✅ All existing tests pass: 93/93 ✅

### Compilation Status
- ✅ Code compiles successfully
- ⚠️ Expected warning: `caption` parameter unused (will be used in Task 3)
- ✅ All tests pass: 93 tests across all modules

## Data Flow
```
Photo Message → handle_photo_message() → msg.caption() → ImageProcessingParams.caption
                                      → download_and_process_image() → [Task 3: Use caption]
```

## Next Steps
Task 2 provides the infrastructure for Task 3, where the `caption` parameter will be used to set the recipe name instead of the hardcoded "Recipe" string.

**Ready to proceed with Task 3: Core Logic Implementation** 🚀