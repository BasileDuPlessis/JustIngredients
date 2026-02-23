# OCR Quantity Recovery & Fraction Post-Processing Specification

## 1. Problem Statement
Tesseract OCR frequently struggles with recognizing fractions (e.g., "1/2", "¾") at the beginning of lines in recipe images. This results in extracted text like "cup of flour" instead of "1/2 cup of flour". 

When the quantity is missed or misinterpreted as an "absurd" value (e.g., letters like "I/Z" or unusually large numbers), the resulting ingredient data is incomplete, requiring manual correction by the user.

## 2. Proposed Evolution
To address this, we will implement a two-tier post-processing recovery system:
1. **Automated Recovery (Zone-Based OCR Retry)**: Detect missing/absurd quantities, isolate the specific image zone where the quantity should be, apply targeted preprocessing, and retry OCR with highly constrained parameters.
2. **Interactive Fallback (User Prompt)**: If automated recovery fails, default the quantity to `0` or `?` and explicitly prompt the user to correct it via the Telegram bot interface.

---

## 3. Tier 1: Automated Zone-Based OCR Retry

### 3.1. Detection Mechanism
After the initial full-image OCR pass, the text is processed by the `MeasurementDetector` (in `src/text_processing.rs`). We will flag an ingredient line for recovery if:
- **Missing Quantity**: A known unit (cup, tbsp, etc.) and ingredient name are found, but no leading number/fraction is detected.
- **Absurd Quantity**: The parsed quantity contains suspicious characters (e.g., "l/2", "I/Z") or is unrealistically large for the detected unit (e.g., "100 cups").

### 3.2. Bounding Box Extraction
To re-process the specific zone, we need the spatial coordinates of the text line.
- **Implementation**: Modify the initial OCR pass in `src/ocr.rs` to output **HOCR** (HTML OCR) or use Tesseract's `ResultIterator` via the `leptess` crate.
- This allows us to map the flagged text line back to its `(x, y, width, height)` bounding box on the original image.

### 3.3. Zone Isolation & Targeted Preprocessing
Once the bounding box of the problematic line is identified:
1. **Crop the Target Zone**: Extract a region corresponding to the left-most 15-25% of the line's bounding box (where the quantity typically resides), adding a small padding margin.
2. **Aggressive Preprocessing**: Apply preprocessing specifically tuned for small, dense text (fractions):
   - **Upscaling**: Scale the cropped zone by 2x or 3x using cubic interpolation.
   - **Binarization**: Apply adaptive thresholding or Otsu's method to maximize contrast.
   - **Morphological Operations**: Apply slight dilation to connect broken fraction strokes (e.g., connecting the numerator, slash, and denominator).

### 3.4. Constrained OCR Pass
Run a secondary Tesseract instance exclusively on the preprocessed cropped image:
- **PSM (Page Segmentation Mode)**: Set to `PSM 8` (Treat the image as a single word) or `PSM 7` (Treat the image as a single text line).
- **Character Whitelist**: Restrict the OCR engine to only recognize numbers and fraction characters: `0123456789/½⅓⅔¼¾⅕⅖⅚⅙⅛⅜⅝⅞. `
- If a valid quantity is found, update the `MeasurementMatch`.

---

## 4. Tier 2: Interactive User Fallback

If the Zone-Based OCR Retry still fails to yield a valid quantity, the system will gracefully degrade to user interaction.

### 4.1. Data Model Updates
- Update `MeasurementMatch` in `src/text_processing.rs` to include a flag: `requires_quantity_confirmation: bool`.
- Set the `quantity` field to `"0"` or a placeholder like `"?"`.

### 4.2. Telegram UI/UX Flow (`src/bot/ui_builder.rs` & `dialogue.rs`)
When presenting the extracted ingredients to the user for review:
1. **Visual Highlighting**: Highlight ingredients missing quantities with a warning emoji (e.g., `⚠️ ? cup of flour`).
2. **Forced Interaction**: Modify the post-confirmation workflow. If any ingredient has `requires_quantity_confirmation == true`, the bot will automatically trigger the "Edit Ingredient" flow for that specific item before allowing the user to save the recipe.
3. **Targeted Prompt**: Send a localized message: 
   > *"We found 'cup of flour' but couldn't read the exact amount. Please type the quantity (e.g., '1/2' or '2'):"*

---

## 5. Implementation Plan & Architecture

### Phase 1: Anomaly Detection & Fallback UI (High ROI, Low Effort)
1. **Update `MeasurementDetector`**: Add logic to identify lines with units/ingredients but missing quantities.
2. **Update Data Models**: Add the `requires_quantity_confirmation` flag.
3. **Update Bot Dialogue**: Implement the UI highlighting and forced prompt for missing quantities.
*Benefit: Immediately solves the data integrity issue by keeping the human in the loop.*

### Phase 2: HOCR & Bounding Box Support (Medium Effort)
1. **Modify `ocr.rs`**: Switch from `get_utf8_text()` to `get_hocr_text(0)` or implement a `ResultIterator` traversal.
2. **HOCR Parser**: Write a lightweight parser to extract line text and bounding box coordinates (`title="bbox x0 y0 x1 y1"`).
3. **Mapping**: Correlate the text lines processed by `MeasurementDetector` with their spatial bounding boxes.

### Phase 3: Targeted Preprocessing & Re-OCR (High Effort)
1. **Image Cropping**: Use the `image` crate to crop the original image based on the left portion of the bounding box.
2. **Preprocessing Pipeline**: Add a specialized fraction-enhancement pipeline in `src/preprocessing/`.
3. **Secondary OCR**: Instantiate a new Tesseract call with the strict whitelist and PSM 8.
4. **Integration**: Merge the recovered quantity back into the `MeasurementMatch`.

---

## 6. Trade-offs & Considerations

| Aspect | Consideration | Mitigation |
| :--- | :--- | :--- |
| **Performance** | Running a second OCR pass adds latency. | Only run the second pass on small, cropped zones, which takes milliseconds. Only trigger when an anomaly is detected. |
| **Complexity** | Parsing HOCR and mapping text back to image coordinates is complex. | Use a robust XML/HTML parser for HOCR, or rely strictly on Tesseract's C-API `ResultIterator` via `leptess`. |
| **User Fatigue** | Prompting the user too often degrades the UX. | The automated Zone-Based retry (Tier 1) acts as a shield to minimize how often Tier 2 (User Prompt) is actually needed. |
| **Memory** | Keeping the original image in memory for cropping. | The image is already temporarily saved to disk; we can reload just the required crop using `image::io::Reader`. |