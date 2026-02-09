use std::fs;
use std::path::Path;
use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configuration
    let training_dir = "tmp/training_data";
    let output_dir = "tmp/tesseract_training_output";
    let lang_name = "recipe_font";

    // Create output directory
    fs::create_dir_all(output_dir)?;

    println!("Step 1: Generating training data from box files...");

    // Find all .box files
    let box_files: Vec<_> = fs::read_dir(training_dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|s| s.to_str()) == Some("box"))
        .collect();

    if box_files.is_empty() {
        eprintln!("ERROR: No .box files found in {} directory", training_dir);
        std::process::exit(1);
    }

    // Process each box file
    for box_file in &box_files {
        let stem = box_file
            .file_stem()
            .expect("box file should have a stem")
            .to_str()
            .expect("stem should be valid UTF-8");
        let tif_file = box_file.with_extension("tif");

        if !tif_file.exists() {
            println!("WARNING: Missing .tif file for {}", box_file.display());
            continue;
        }

        // tesseract image.tif image --psm 6 nobatch box.train
        let output_base = format!("{}/{}", training_dir, stem);
        let status = Command::new("tesseract")
            .arg(&tif_file)
            .arg(&output_base)
            .arg("--psm")
            .arg("6")
            .arg("nobatch")
            .arg("box.train")
            .status()?;

        if !status.success() {
            eprintln!("ERROR: Failed to generate training data for {}", stem);
            std::process::exit(1);
        }

        println!("SUCCESS: Generated training data for {}", stem);
    }

    println!("\nStep 2: Creating unicharset...");

    let unicharset_path = format!("{}/unicharset", output_dir);
    let mut cmd = Command::new("unicharset_extractor");
    for box_file in &box_files {
        cmd.arg(box_file);
    }
    cmd.arg(&unicharset_path);

    let status = cmd.status()?;
    if !status.success() {
        eprintln!("ERROR: Failed to extract unicharset");
        std::process::exit(1);
    }
    println!("SUCCESS: Extracted unicharset");

    println!("\nStep 3: Creating clustering data...");

    // Find all .tr files
    let tr_files: Vec<_> = fs::read_dir(training_dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|s| s.to_str()) == Some("tr"))
        .collect();

    // Shape clustering
    let shapetable_path = format!("{}/shapetable", output_dir);
    let mut cmd = Command::new("shapeclustering");
    cmd.arg("-F")
        .arg(format!("{}/font_properties", training_dir))
        .arg("-U")
        .arg(&unicharset_path)
        .arg("-O")
        .arg(&shapetable_path);
    for tr_file in &tr_files {
        cmd.arg(tr_file);
    }

    let status = cmd.status()?;
    if !status.success() {
        eprintln!("ERROR: Shape clustering failed");
        std::process::exit(1);
    }
    println!("SUCCESS: Shape clustering completed");

    // MF training
    let mut cmd = Command::new("mftraining");
    cmd.arg("-F")
        .arg(format!("{}/font_properties", training_dir))
        .arg("-U")
        .arg(&unicharset_path)
        .arg("-O")
        .arg(format!("{}.unicharset", lang_name));
    for tr_file in &tr_files {
        cmd.arg(tr_file);
    }

    let status = cmd.status()?;
    if !status.success() {
        eprintln!("ERROR: MF training failed");
        std::process::exit(1);
    }
    println!("SUCCESS: MF training completed");

    // CN training
    let mut cmd = Command::new("cntraining");
    for tr_file in &tr_files {
        cmd.arg(tr_file);
    }

    let status = cmd.status()?;
    if !status.success() {
        eprintln!("ERROR: CN training failed");
        std::process::exit(1);
    }
    println!("SUCCESS: CN training completed");

    println!("\nStep 4: Combining trained data...");

    // Rename files to match language prefix
    let file_names = ["inttemp", "normproto", "pffmtable"];
    for file_name in &file_names {
        let src_path = Path::new(file_name);
        if src_path.exists() {
            let new_name = format!("{}.{}", lang_name, file_name);
            let dst_path = Path::new(output_dir).join(&new_name);
            fs::rename(src_path, &dst_path)?;
        }
    }

    // Rename shapetable
    let shapetable = Path::new(output_dir).join("shapetable");
    if shapetable.exists() {
        let new_shapetable = format!("{}.shapetable", lang_name);
        let new_path = Path::new(output_dir).join(&new_shapetable);
        fs::rename(shapetable, &new_path)?;
    }

    // Rename unicharset
    let unicharset_src = Path::new(output_dir).join("unicharset");
    if unicharset_src.exists() {
        let unicharset_file = format!("{}.unicharset", lang_name);
        let unicharset_dst = Path::new(output_dir).join(&unicharset_file);
        fs::rename(unicharset_src, &unicharset_dst)?;
    }

    // Combine tessdata
    let status = Command::new("combine_tessdata")
        .arg(lang_name)
        .current_dir(output_dir)
        .status()?;

    if !status.success() {
        eprintln!("ERROR: Failed to combine tessdata");
        std::process::exit(1);
    }
    println!("SUCCESS: Combined tessdata");

    println!("\nStep 5: Moving final model...");

    let final_model = format!("{}.traineddata", lang_name);
    let final_path = Path::new(output_dir).join(&final_model);

    if final_path.exists() {
        println!("SUCCESS: Model saved to {}", final_path.display());
    } else {
        eprintln!("ERROR: Final model not created");
        std::process::exit(1);
    }

    println!("\n🎉 Training completed successfully!");
    println!("Model: {}", final_path.display());
    println!("You can now use this model with Tesseract:");
    println!("  tesseract image.tif output -l {}", lang_name);

    Ok(())
}
