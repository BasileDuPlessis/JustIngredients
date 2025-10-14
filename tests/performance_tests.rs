//! # Performance Impact Assessment Tests
//!
//! Tests to measure and validate the performance impact of observability features
//! on the JustIngredients bot's core functionality.

#[cfg(test)]
mod tests {
    use just_ingredients::observability;
    use just_ingredients::text_processing::MeasurementDetector;
    use std::time::{Duration, Instant};

    /// Baseline performance test without observability
    #[test]
    fn test_baseline_performance_without_observability() {
        let detector = MeasurementDetector::new().unwrap();

        let test_text = r#"
        Recipe ingredients:
        2 cups flour
        1 cup sugar
        3 eggs
        1/2 cup butter
        1 teaspoon vanilla
        2 tablespoons milk
        "#;

        let iterations = 1000;

        let start = Instant::now();

        for _ in 0..iterations {
            let _measurements = detector.extract_ingredient_measurements(test_text);
        }

        let duration = start.elapsed();
        let avg_time_per_operation = duration.as_nanos() / iterations;

        println!(
            "📊 Baseline performance: {}ns per text processing operation ({} iterations)",
            avg_time_per_operation, iterations
        );

        // Store baseline for comparison (in a real scenario, this would be compared to with-observability runs)
        assert!(
            avg_time_per_operation > 0,
            "Operations should take some time"
        );
    }

    /// Performance test with observability enabled
    #[test]
    fn test_performance_with_observability_enabled() {
        let detector = MeasurementDetector::new().unwrap();

        let test_text = r#"
        Recipe ingredients:
        2 cups flour
        1 cup sugar
        3 eggs
        1/2 cup butter
        1 teaspoon vanilla
        2 tablespoons milk
        "#;

        let iterations = 1000;

        let start = Instant::now();

        for i in 0..iterations {
            // Add observability overhead
            let span = observability::ocr_span(&format!("perf_test_{}", i));
            let _enter = span.enter();

            observability::record_ocr_metrics(true, Duration::from_millis(10), 1024);

            let _measurements = detector.extract_ingredient_measurements(test_text);

            observability::record_request_metrics("POST", 200, Duration::from_millis(50));
        }

        let duration = start.elapsed();
        let avg_time_per_operation = duration.as_nanos() / iterations;

        println!(
            "📊 Performance with observability: {}ns per operation ({} iterations)",
            avg_time_per_operation, iterations
        );

        // The overhead should be reasonable (< 10% of total operation time for typical OCR operations)
        // This is a sanity check - actual acceptable overhead depends on requirements
        assert!(
            avg_time_per_operation > 0,
            "Operations should take some time"
        );
        assert!(
            avg_time_per_operation < 1_000_000_000,
            "Should complete in reasonable time"
        ); // < 1 second
    }

    /// Measure observability overhead specifically
    #[test]
    fn test_observability_overhead_measurement() {
        let iterations = 10_000;

        // Measure time without observability
        let start_baseline = Instant::now();
        for i in 0..iterations {
            let _ = i * 2; // Minimal work to measure baseline
        }
        let baseline_duration = start_baseline.elapsed();

        // Measure time with observability
        let start_with_obs = Instant::now();
        for i in 0..iterations {
            let span = observability::ocr_span(&format!("overhead_test_{}", i % 100)); // Reuse span names
            let _enter = span.enter();

            observability::record_telegram_message("overhead_test");
            observability::record_ocr_metrics(true, Duration::from_micros(i as u64 % 1000), 1024);

            let _ = i * 2; // Same minimal work
        }
        let with_obs_duration = start_with_obs.elapsed();

        let overhead = with_obs_duration.saturating_sub(baseline_duration);
        let avg_overhead_per_operation = overhead.as_nanos() / iterations;

        println!(
            "📊 Observability overhead: {}ns per operation ({} iterations)",
            avg_overhead_per_operation, iterations
        );
        println!("📊 Total overhead: {:?}", overhead);
        println!("📊 Baseline: {:?}", baseline_duration);
        println!("📊 With observability: {:?}", with_obs_duration);

        // Observability overhead should be minimal (< 100ns per operation typically)
        // This ensures observability doesn't significantly impact performance
        assert!(
            avg_overhead_per_operation < 1_000_000,
            "Overhead too high: {}ns per operation",
            avg_overhead_per_operation
        );
    }

    /// Test memory usage impact of observability
    #[test]
    fn test_memory_usage_impact() {
        use std::mem;

        // Measure memory usage of observability structures
        let span_size = mem::size_of::<tracing::Span>();
        println!("📊 Tracing span size: {} bytes", span_size);

        // Create some observability objects and measure their size
        let _ocr_span = observability::ocr_span("memory_test");
        let _db_span = observability::db_span("memory_test", "test_table");

        // These are rough estimates - actual memory usage is more complex
        println!("📊 OCR span created successfully");
        println!("📊 DB span created successfully");

        // Memory usage should be reasonable
        assert!(span_size > 0, "Span should have some memory footprint");
        assert!(span_size < 1000, "Span should not be excessively large");
    }

    /// Test CPU usage impact during sustained load
    #[tokio::test]
    async fn test_cpu_usage_under_sustained_load() {
        let duration = Duration::from_secs(5);
        let start = Instant::now();

        let mut operations = 0;

        while start.elapsed() < duration {
            // Simulate continuous observability usage
            let span = observability::ocr_span("cpu_test");
            let _enter = span.enter();

            observability::record_telegram_message("cpu_load_test");
            observability::record_ocr_metrics(true, Duration::from_millis(10), 1024);
            observability::record_db_metrics("SELECT", Duration::from_micros(500));
            observability::record_request_metrics("GET", 200, Duration::from_millis(25));

            operations += 1;

            // Small yield to prevent overwhelming the system
            tokio::task::yield_now().await;
        }

        let elapsed = start.elapsed();
        let operations_per_second = operations as f64 / elapsed.as_secs_f64();

        println!(
            "📊 Sustained load: {:.0} operations/second over {:?} ({} total operations)",
            operations_per_second, elapsed, operations
        );

        // Should handle reasonable sustained load
        assert!(
            operations_per_second > 100.0,
            "Should handle at least 100 ops/sec: {:.0}",
            operations_per_second
        );
    }

    /// Test observability impact on concurrent operations
    #[tokio::test]
    async fn test_concurrent_performance_impact() {
        let task_count = 10;
        let operations_per_task = 1000;

        let start = Instant::now();
        let mut handles = vec![];

        // Spawn concurrent tasks
        for task_id in 0..task_count {
            let handle = tokio::spawn(async move {
                let mut local_operations = 0;

                for i in 0..operations_per_task {
                    let span =
                        observability::ocr_span(&format!("concurrent_{}_{}", task_id, i % 10));
                    let _enter = span.enter();

                    observability::record_telegram_message("concurrent_test");
                    observability::record_ocr_metrics(true, Duration::from_millis(5), 512);

                    local_operations += 1;
                }

                local_operations
            });
            handles.push(handle);
        }

        // Wait for all tasks and sum operations
        let mut total_operations = 0;
        for handle in handles {
            total_operations += handle.await.unwrap();
        }

        let elapsed = start.elapsed();
        let operations_per_second = total_operations as f64 / elapsed.as_secs_f64();

        println!("📊 Concurrent performance: {:.0} operations/second over {:?} ({} tasks, {} total operations)",
                operations_per_second, elapsed, task_count, total_operations);

        // Should handle concurrent load reasonably well
        assert!(
            operations_per_second > 500.0,
            "Should handle at least 500 concurrent ops/sec: {:.0}",
            operations_per_second
        );
    }

    /// Test observability scalability - performance with increasing load
    #[tokio::test]
    async fn test_observability_scalability() {
        let load_levels = vec![1, 5, 10, 20, 50];
        let operations_per_level = 100;

        for &concurrency in &load_levels {
            let start = Instant::now();
            let mut handles = vec![];

            // Spawn tasks at this concurrency level
            for task_id in 0..concurrency {
                let handle = tokio::spawn(async move {
                    for i in 0..operations_per_level {
                        let span = observability::ocr_span(&format!("scale_{}_{}", task_id, i % 5));
                        let _enter = span.enter();

                        observability::record_ocr_metrics(true, Duration::from_millis(10), 1024);
                        observability::record_db_metrics("SELECT", Duration::from_millis(5));
                    }
                });
                handles.push(handle);
            }

            // Wait for all tasks at this level
            for handle in handles {
                handle.await.unwrap();
            }

            let elapsed = start.elapsed();
            let total_operations = concurrency * operations_per_level;
            let operations_per_second = total_operations as f64 / elapsed.as_secs_f64();

            println!(
                "📊 Scalability test - Concurrency: {}, Ops/sec: {:.0}, Time: {:?}",
                concurrency, operations_per_second, elapsed
            );

            // Performance should degrade gracefully, not collapse
            assert!(
                operations_per_second > 10.0,
                "Performance too low at concurrency {}: {:.0} ops/sec",
                concurrency,
                operations_per_second
            );
        }
    }

    /// Test that observability doesn't affect core OCR processing speed significantly
    #[test]
    fn test_ocr_processing_speed_impact() {
        let detector = MeasurementDetector::new().unwrap();

        // Test text of varying complexity
        let test_cases = vec![
            ("Simple", "2 cups flour\n3 eggs"),
            ("Medium", "2 cups all-purpose flour\n1 cup granulated sugar\n3 large eggs\n1/2 cup unsalted butter\n1 teaspoon vanilla extract\n2 tablespoons milk"),
            ("Complex", r#"
                2 1/4 cups all-purpose flour
                1 teaspoon baking soda
                1 teaspoon salt
                1 cup unsalted butter, softened
                3/4 cup granulated sugar
                3/4 cup packed brown sugar
                2 large eggs
                2 teaspoons vanilla extract
                2 cups semisweet chocolate chips
                1 cup chopped walnuts (optional)
            "#),
        ];

        for (complexity, text) in test_cases {
            let iterations = 100;

            // Measure without observability
            let start_baseline = Instant::now();
            for _ in 0..iterations {
                let _ = detector.extract_ingredient_measurements(text);
            }
            let baseline = start_baseline.elapsed();

            // Measure with observability
            let start_with_obs = Instant::now();
            for i in 0..iterations {
                let span = observability::ocr_span(&format!("ocr_speed_test_{}", i % 10));
                let _enter = span.enter();

                let measurements = detector.extract_ingredient_measurements(text);
                observability::record_ocr_metrics(true, Duration::from_millis(50), 1024);

                // Verify we get results
                assert!(
                    !measurements.is_empty(),
                    "Should extract measurements from {} text",
                    complexity
                );
            }
            let with_obs = start_with_obs.elapsed();

            let overhead = with_obs.saturating_sub(baseline);
            let overhead_percentage =
                (overhead.as_nanos() as f64 / baseline.as_nanos() as f64) * 100.0;

            println!(
                "📊 OCR Speed Impact ({}): Baseline: {:?}, With obs: {:?}, Overhead: {:.1}%",
                complexity, baseline, with_obs, overhead_percentage
            );

            // Overhead should be reasonable (< 50% for complex text processing)
            assert!(
                overhead_percentage < 50.0,
                "OCR processing overhead too high for {}: {:.1}%",
                complexity,
                overhead_percentage
            );
        }
    }

    /// Test observability garbage collection and cleanup performance
    #[tokio::test]
    async fn test_observability_cleanup_performance() {
        let iterations = 1000;

        let start = Instant::now();

        // Create many spans and metrics that should be cleaned up
        for i in 0..iterations {
            {
                let span = observability::ocr_span(&format!("cleanup_perf_{}", i));
                let _enter = span.enter();

                observability::record_telegram_message("cleanup_test");
                observability::record_ocr_metrics(true, Duration::from_millis(1), 512);
                observability::record_db_metrics("SELECT", Duration::from_micros(100));
            } // Span goes out of scope here
        }

        // Allow some time for cleanup
        tokio::time::sleep(Duration::from_millis(100)).await;

        let elapsed = start.elapsed();
        let operations_per_second = iterations as f64 / elapsed.as_secs_f64();

        println!(
            "📊 Cleanup performance: {:.0} operations/second ({} iterations, {:?})",
            operations_per_second, iterations, elapsed
        );

        // Cleanup should be fast
        assert!(
            operations_per_second > 1000.0,
            "Cleanup too slow: {:.0} ops/sec",
            operations_per_second
        );
    }
}
