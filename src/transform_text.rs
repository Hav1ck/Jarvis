use anyhow::{Context, Result, bail};
use arboard::Clipboard;
use base64::{Engine as _, engine::general_purpose};
use png::{BitDepth, ColorType, Encoder};
use regex::Regex;

// copies text to clipboard between [[copy]] and [[/copy]] tags
pub fn copy_to_clipboard_function_for_llm(text: &str) -> Result<String> {
    println!("[DEBUG] Entered copy_to_clipboard_function_for_llm");
    let re = Regex::new(r"(?s)\[\[copy\]\](.*?)\[\[/copy\]\]")
        .context("Failed to compile copy regex")?;

    let mut clipboard = Clipboard::new().context("Failed to initialize clipboard")?;

    if let Some(cap) = re.captures(text) {
        let content = cap.get(1).map(|m| m.as_str()).unwrap_or_default();
        clipboard
            .set_text(content.to_string())
            .context("Failed to set clipboard text")?;
        println!("[DEBUG] Finished copy_to_clipboard_function_for_llm");
        return Ok(re.replace_all(text, "").into_owned());
    } else {
        println!("[DEBUG] Finished copy_to_clipboard_function_for_llm");
        return Ok(text.to_string());
    }
}

// pastes clipboard content into the prompt if it contains a paste command
pub fn paste_clipboard_instead_of_text(prompt: &str) -> Result<String> {
    println!("[DEBUG] Entered paste_clipboard_instead_of_text");
    let re = Regex::new(
        r"(?i)\b(clipboard|ctrl\+v|control v|flipboards|paste|control-v|control+v|steuerung V|flipboard)\b",
    )
    .context("Failed to compile paste regex")?;

    if re.is_match(prompt) {
        let mut clipboard = Clipboard::new().context("Failed to initialize clipboard")?;

        // tries to paste text
        if let Ok(txt) = clipboard.get_text() {
            println!("[DEBUG] Finished paste_clipboard_instead_of_text (text)");
            return Ok(re.replace_all(prompt, &txt).into_owned());
        }

        // if could not paste text, tries to paste image
        if let Ok(img) = clipboard.get_image() {
            let mut buf = Vec::new();
            {
                let mut encoder = Encoder::new(&mut buf, img.width as u32, img.height as u32);
                encoder.set_color(ColorType::Rgba);
                encoder.set_depth(BitDepth::Eight);
                let mut writer = encoder
                    .write_header()
                    .context("Failed to write PNG header")?;
                writer
                    .write_image_data(&img.bytes)
                    .context("Failed to write PNG data")?;
            }

            let b64 = general_purpose::STANDARD.encode(&buf);
            let data_uri = format!("data:image/png;base64,{}", b64);
            println!("[DEBUG] Finished paste_clipboard_instead_of_text (image)");
            return Ok(re.replace_all(prompt, &data_uri).into_owned());
        }

        bail!("Clipboard is empty or has unsupported content type");
    }

    println!("[DEBUG] Finished paste_clipboard_instead_of_text (no match)");
    Ok(prompt.to_string())
}

// extracts image parts from a prompt if it contains a data URI
pub fn extract_image_parts(prompt: &str) -> Result<(String,), (String, String, Vec<u8>, String)> {
    println!("[DEBUG] Entered extract_image_parts");
    let re = Regex::new(
        r#"(?P<pre>.*?)(?P<uri>data:image/(?P<mime>\w+);base64,(?P<b64>[A-Za-z0-9+/=]+))(?P<post>.*)"#,
    )
    .expect("Failed to compile image extraction regex");

    if let Some(cap) = re.captures(prompt) {
        let pre = cap.name("pre").unwrap().as_str().to_string();
        let mime = cap.name("mime").unwrap().as_str().to_string();
        let b64 = cap.name("b64").unwrap().as_str();
        let post = cap.name("post").unwrap().as_str().to_string();

        let bytes = general_purpose::STANDARD
            .decode(b64)
            .map_err(|_| (pre.clone(), mime.clone(), Vec::new(), post.clone()))?;

        println!("[DEBUG] Finished extract_image_parts (found image)");
        return Err((pre, mime, bytes, post));
    } else {
        println!("[DEBUG] Finished extract_image_parts (no image)");
        return Ok((prompt.to_string(),));
    }
}
