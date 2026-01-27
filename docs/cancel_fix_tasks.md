# Tasks to Fix: Cancel Button After Image Upload

## Issue Description
When a user uploads a picture and then clicks the cancel button, the OCR processing should be cancelled and all related UI elements (buttons and preview) should be removed.

## Tasks

### ✅ 1. Implement Cancel Button Handler
- ✅ Add a cancel button to the OCR processing UI
- ✅ Handle the cancel callback to stop any ongoing OCR processing
- ✅ Ensure the cancel action is properly registered in the dialogue state

### ✅ 2. Remove OCR Preview and Buttons
- ✅ When cancel is clicked, remove the OCR text preview
- ✅ Remove all action buttons (Add Another, List Recipes, Search, etc.)
- ✅ Clear any temporary OCR data from the dialogue state

### ✅ 3. Update Dialogue State Management
- ✅ Modify `dialogue.rs` to handle cancellation states
- ✅ Ensure cancelled OCR entries are not saved to the database
- ✅ Reset dialogue to a clean state after cancellation

### ✅ 4. UI Cleanup
- ✅ Update `bot.rs` message handling to remove previous messages when cancelled
- ✅ Ensure no orphaned messages remain in the chat
- ✅ Provide user feedback that the operation was cancelled

### ✅ 5. Testing
- ✅ Add integration tests for the cancel functionality
- ✅ Test that OCR processing is properly interrupted
- ✅ Verify UI elements are removed correctly

## Implementation Summary

### Changes Made:
1. **UI Components**: Added `create_processing_keyboard()` function with cancel button
2. **Image Processing**: Modified to show cancel button during OCR processing
3. **Callback Handling**: Added `handle_cancel_processing_button()` for processing phase cancellation
4. **Review Cancellation**: Updated `handle_cancel_review_button()` to edit existing message instead of sending new one
5. **Localization**: Added "processing-cancelled" keys in English and French
6. **Code Quality**: All tests pass, clippy clean, properly formatted

### Key Features:
- Cancel button appears immediately when image processing starts
- Clicking cancel stops the operation and removes all UI elements
- No orphaned messages or buttons remain in the chat
- Proper user feedback with localized messages
- Dialogue state is properly cleaned up