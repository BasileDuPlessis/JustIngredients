use just_ingredients::text_processing::MeasurementDetector;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let detector = MeasurementDetector::new()?;

    println!("=== CURRENT REGEX BEHAVIOR ANALYSIS ===\n");

    // Test cases that demonstrate the alternation problem
    let test_cases = vec![
        ("2 crème fraîche", "quantity + multi-word, no measurement"),
        ("6 pommes de terre", "quantity + multi-word, no measurement"),
        ("2g de chocolat", "quantity + measurement + preposition + name"),
        ("500g chocolat noir", "quantity + measurement + multi-word name"),
        ("3 eggs", "quantity + single word, no measurement"),
        ("1 cup flour", "quantity + measurement + single word"),
    ];

    println!("Current Regex Pattern:");
    println!("{:?}\n", detector.pattern_str());

    for (input, description) in test_cases {
        println!("Input: '{}' ({})", input, description);
        let matches = detector.extract_ingredient_measurements(input);

        if matches.is_empty() {
            println!("  ❌ No matches found");
        } else {
            for (i, m) in matches.iter().enumerate() {
                println!("  Match {}: quantity='{}', measurement={:?}, ingredient_name='{}'",
                    i + 1, m.quantity, m.measurement, m.ingredient_name);
            }
        }
        println!();
    }

    Ok(())
}