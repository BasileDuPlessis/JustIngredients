//! Callbacks module for handling all inline keyboard callback queries
//!
//! This module is organized into submodules for different types of callbacks:
//! - `callback_handler`: Main routing handler for all callback queries
//! - `callback_types`: Shared parameter structs for callback handlers
//! - `recipe_callbacks`: Recipe selection, actions, and management
//! - `workflow_callbacks`: Workflow transitions and navigation
//! - `review_callbacks`: ReviewIngredients dialogue state handlers
//! - `editing_callbacks`: EditingSavedIngredients dialogue state handlers

pub mod callback_handler;
pub mod callback_types;
pub mod editing_callbacks;
pub mod recipe_callbacks;
pub mod review_callbacks;
pub mod workflow_callbacks;