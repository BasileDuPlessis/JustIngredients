//! # OCR Instance Manager Module
//!
//! This module provides thread-safe OCR instance management for reusing Tesseract instances.
//! Reusing instances significantly improves performance by avoiding initialization overhead.

use leptess::LepTess;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::info;

use crate::ocr_config::OcrConfig;

/// Errors that can occur in the OCR instance manager
#[derive(Debug, thiserror::Error)]
pub enum InstanceManagerError {
    #[error("Failed to acquire instances lock: {0}")]
    LockError(String),

    #[error("Failed to initialize Tesseract OCR instance: {0}")]
    TesseractInitError(String),

    #[error("Failed to set PSM mode: {0}")]
    PsmModeError(String),

    #[error("Failed to set user words file: {0}")]
    UserWordsError(String),

    #[error("Failed to set user patterns file: {0}")]
    UserPatternsError(String),

    #[error("Failed to set character whitelist: {0}")]
    CharacterWhitelistError(String),
}

/// Thread-safe OCR instance manager for reusing Tesseract instances
///
/// Manages a pool of Tesseract OCR instances keyed by language configuration.
/// Reusing instances significantly improves performance by avoiding the overhead
/// of creating new Tesseract instances for each OCR operation.
///
/// # Performance Benefits
///
/// - Eliminates Tesseract initialization overhead (~100-500ms per instance)
/// - Reduces memory allocations for repeated OCR operations
/// - Thread-safe with Arc<Mutex<>> for concurrent access
///
/// # Instance Lifecycle
///
/// - Instances are created on first request for a language combination
/// - Instances are reused for subsequent requests with same language config
/// - Instances persist until explicitly removed or manager is dropped
///
/// # Thread Safety
///
/// Uses `Mutex<HashMap<>>` internally for thread-safe instance management.
/// Multiple threads can safely request instances concurrently.
///
/// # Memory Management
///
/// - Each language combination maintains one instance
/// - Memory usage scales with number of unique language combinations
/// - Consider memory limits for applications with many language combinations
pub struct OcrInstanceManager {
    instances: Mutex<HashMap<String, Arc<Mutex<LepTess>>>>,
}

impl OcrInstanceManager {
    /// Create a new OCR instance manager
    ///
    /// Initializes an empty instance pool. Instances will be created
    /// on-demand when first requested via `get_instance()`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use just_ingredients::instance_manager::OcrInstanceManager;
    ///
    /// let manager = OcrInstanceManager::new();
    /// // Manager is ready to provide OCR instances
    /// ```
    pub fn new() -> Self {
        Self {
            instances: Mutex::new(HashMap::new()),
        }
    }

    /// Get or create an OCR instance for the given configuration
    ///
    /// Returns an existing instance if one exists for the language configuration,
    /// otherwise creates a new instance and stores it for future reuse.
    ///
    /// # Arguments
    ///
    /// * `config` - OCR configuration containing language settings and other options
    ///
    /// # Returns
    ///
    /// Returns `Result<Arc<Mutex<LepTess>>, InstanceManagerError>` containing the OCR instance
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use just_ingredients::instance_manager::OcrInstanceManager;
    /// use just_ingredients::ocr_config::OcrConfig;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = OcrInstanceManager::new();
    /// let config = OcrConfig::default();
    ///
    /// let instance = manager.get_instance(&config)?;
    /// // Use the instance for OCR processing
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns error if Tesseract instance creation fails (e.g., invalid language codes)
    /// or if mutex lock acquisition fails
    ///
    /// # Performance
    ///
    /// - First call for a language: ~100-500ms (Tesseract initialization)
    /// - Subsequent calls: ~1ms (instance lookup and Arc clone)
    pub fn get_instance(
        &self,
        config: &OcrConfig,
    ) -> Result<Arc<Mutex<LepTess>>, InstanceManagerError> {
        // Create a unique key that includes both languages and model type
        let key = format!("{}:{}", config.languages, config.model_type.tessdata_dir());

        // Try to get existing instance
        {
            let instances = self
                .instances
                .lock()
                .map_err(|e| InstanceManagerError::LockError(e.to_string()))?;
            if let Some(instance) = instances.get(&key) {
                return Ok(Arc::clone(instance));
            }
        }

        // Create new instance if none exists
        info!(
            "Creating new OCR instance for languages: {} with model: {}",
            config.languages,
            config.model_type.tessdata_dir()
        );

        // Determine tessdata path based on model type
        let tessdata_path = Self::get_tessdata_path(config.model_type);

        let mut tess = LepTess::new(tessdata_path.as_deref(), &config.languages)
            .map_err(|e| InstanceManagerError::TesseractInitError(e.to_string()))?;

        // Set default PSM mode (can be overridden later)
        tess.set_variable(
            leptess::Variable::TesseditPagesegMode,
            config.psm_mode.as_str(),
        )
        .map_err(|e| InstanceManagerError::PsmModeError(e.to_string()))?;

        // Set custom user words file if configured
        if let Some(user_words_path) = &config.user_words_file {
            tess.set_variable(leptess::Variable::UserWordsFile, user_words_path)
                .map_err(|e| InstanceManagerError::UserWordsError(e.to_string()))?;
            info!(
                "Configured Tesseract with custom user words file: {}",
                user_words_path
            );
        }

        // Set custom user patterns file if configured
        if let Some(user_patterns_path) = &config.user_patterns_file {
            tess.set_variable(leptess::Variable::UserPatternsFile, user_patterns_path)
                .map_err(|e| InstanceManagerError::UserPatternsError(e.to_string()))?;
            info!(
                "Configured Tesseract with custom user patterns file: {}",
                user_patterns_path
            );
        }

        // Set character whitelist if configured
        if let Some(whitelist) = &config.character_whitelist {
            tess.set_variable(leptess::Variable::TesseditCharWhitelist, whitelist)
                .map_err(|e| InstanceManagerError::CharacterWhitelistError(e.to_string()))?;
            info!(
                "Configured Tesseract with character whitelist: {} characters",
                whitelist.len()
            );
        }

        let instance = Arc::new(Mutex::new(tess));

        // Store the instance
        {
            let mut instances = self
                .instances
                .lock()
                .map_err(|e| InstanceManagerError::LockError(e.to_string()))?;
            instances.insert(key, Arc::clone(&instance));
        }

        Ok(instance)
    }

    /// Get the tessdata path for the specified model type
    ///
    /// Attempts to find the appropriate tessdata directory based on the model type.
    /// Falls back to default path if specific model directory is not found.
    fn get_tessdata_path(model_type: crate::ocr_config::ModelType) -> Option<String> {
        use crate::ocr_config::ModelType;

        // Common tessdata installation paths to try
        let possible_paths = match model_type {
            ModelType::Fast => vec![
                "/usr/share/tesseract-ocr/5/tessdata_fast",
                "/usr/share/tesseract-ocr/4.00/tessdata_fast",
                "/usr/share/tessdata_fast",
                "/usr/local/share/tessdata_fast",
            ],
            ModelType::Best => vec![
                "/usr/share/tesseract-ocr/5/tessdata_best",
                "/usr/share/tesseract-ocr/4.00/tessdata_best",
                "/usr/share/tessdata_best",
                "/usr/local/share/tessdata_best",
            ],
        };

        // Try each path and return the first one that exists
        for path in possible_paths {
            if std::path::Path::new(path).exists() {
                info!("Using tessdata path: {}", path);
                return Some(path.to_string());
            }
        }

        // Fall back to default (None) if no specific path found
        info!(
            "No specific tessdata path found for model type {:?}, using default",
            model_type
        );
        None
    }

    /// Remove an instance (useful for cleanup or when configuration changes)
    pub fn _remove_instance(
        &self,
        languages: &str,
        model_type: crate::ocr_config::ModelType,
    ) -> Result<(), InstanceManagerError> {
        let key = format!("{}:{}", languages, model_type.tessdata_dir());
        let mut instances = self
            .instances
            .lock()
            .map_err(|e| InstanceManagerError::LockError(e.to_string()))?;
        if instances.remove(&key).is_some() {
            info!(
                "Removed OCR instance for languages: {} with model: {}",
                languages,
                model_type.tessdata_dir()
            );
        }
        Ok(())
    }

    /// Clear all instances (useful for memory cleanup)
    pub fn _clear_all_instances(&self) -> Result<(), InstanceManagerError> {
        let mut instances = self
            .instances
            .lock()
            .map_err(|e| InstanceManagerError::LockError(e.to_string()))?;
        let count = instances.len();
        instances.clear();
        if count > 0 {
            info!("Cleared {count} OCR instances");
        }
        Ok(())
    }

    /// Get the number of cached instances
    pub fn _instance_count(&self) -> Result<usize, InstanceManagerError> {
        let instances = self
            .instances
            .lock()
            .map_err(|e| InstanceManagerError::LockError(e.to_string()))?;
        Ok(instances.len())
    }
}

impl Default for OcrInstanceManager {
    fn default() -> Self {
        Self::new()
    }
}
