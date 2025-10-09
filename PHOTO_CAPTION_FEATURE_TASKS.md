# Feature: Photo Caption as Recipe Name Candidate

## Overview
Implement functionality allowing users to associate a message (caption) with uploaded photos. When present, the bot should use this caption as a candidate for the recipe name, which can still be edited later like other ingredients.

## Task Breakdown

### **1. Analysis & Design**
- **Task 1.1**: Analyze current photo processing flow in `handle_photo_message()` and `download_and_process_image()`
- **Task 1.2**: Identify where recipe name "Recipe" is set as default in dialogue state transition
- **Task 1.3**: Research Telegram API for photo captions (`msg.caption()`) and document behavior
- **Task 1.4**: Design validation rules for caption as recipe name (length limits, sanitization)

### **2. Data Structure Updates**
- **Task 2.1**: Add `caption: Option<String>` field to `ImageProcessingParams` struct
- **Task 2.2**: Update all `ImageProcessingParams` instantiations to include caption field
- **Task 2.3**: Handle caption extraction in both photo and document message handlers

### **3. Core Logic Implementation**
- **Task 3.1**: Modify recipe name assignment logic to use `caption.unwrap_or("Recipe")`
- **Task 3.2**: Ensure caption validation using existing `validate_recipe_name()` function
- **Task 3.3**: Preserve existing editability - user can still change recipe name during review phase
- **Task 3.4**: Handle edge cases: empty captions, very long captions, special characters

### **4. User Experience**
- **Task 4.1**: Update help messages to mention caption feature in `/help` command
- **Task 4.2**: Add localization strings for caption-related messages
- **Task 4.3**: Consider UI feedback when caption is used vs default name

### **5. Testing & Quality Assurance**
- **Task 5.1**: Add unit tests for caption extraction and processing
- **Task 5.2**: Add integration tests for photo-with-caption workflow
- **Task 5.3**: Test edge cases: empty caption, long caption, special characters
- **Task 5.4**: Ensure existing functionality still works (photos without captions)

### **6. Documentation Updates**
- **Task 6.1**: Update README.md with caption feature description
- **Task 6.2**: Update copilot-instructions.md with new feature details
- **Task 6.3**: Add code comments explaining caption handling logic

## Acceptance Criteria
- ✅ Users can send photos with captions
- ✅ Caption is used as initial recipe name suggestion
- ✅ Users can still edit recipe name during review phase
- ✅ Fallback to "Recipe" when no caption provided
- ✅ All existing functionality preserved
- ✅ Comprehensive test coverage
- ✅ Updated documentation

## Dependencies
- Requires existing recipe name validation logic
- Depends on current dialogue state management
- Uses existing localization system

## Risk Assessment
- **Low Risk**: Feature builds on existing photo processing pipeline
- **Backward Compatible**: Photos without captions work exactly as before
- **Minimal Changes**: Only adds caption handling, doesn't modify core OCR logic

## Implementation Notes
- Use `msg.caption()` to extract photo captions from Telegram API
- Integrate with existing `validate_recipe_name()` function for input validation
- Maintain backward compatibility for photos without captions
- Follow existing code patterns and error handling