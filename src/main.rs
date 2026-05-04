use std::fs;
use std::path::Path;
use colored::Colorize;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).unwrap_or(());

    let mut args: Vec<String> = std::env::args().collect();
    
    // Check for -i or --inline flag
    let mut inline_mode = false;
    if let Some(idx) = args.iter().position(|x| x == "-i" || x == "--inline") {
        inline_mode = true;
        args.remove(idx);
    }

    // Check for -o or --output flag
    let mut stdout_mode = false;
    if let Some(idx) = args.iter().position(|x| x == "-o" || x == "--output") {
        stdout_mode = true;
        args.remove(idx);
    }

    // Check for -dp or --decimal-precision
    let mut decimal_precision = None;
    if let Some(idx) = args.iter().position(|x| x == "-dp" || x == "--decimal-precision") {
        if idx + 1 < args.len() {
            if let Ok(val) = args[idx + 1].parse::<u8>() {
                if val <= 4 {
                    decimal_precision = Some(val);
                    args.remove(idx); // Remove flag
                    args.remove(idx); // Remove value (which shifted to idx)
                } else {
                    eprintln!("Error: decimal precision must be between 0 and 4.");
                    std::process::exit(1);
                }
            } else {
                eprintln!("Error: invalid decimal precision value.");
                std::process::exit(1);
            }
        } else {
            eprintln!("Error: missing value for decimal precision.");
            std::process::exit(1);
        }
    }

    if args.len() < 2 {
        eprintln!(
            "Usage: {} [OPTIONS] <input.svg> [output.svg]\n\n\
             Options:\n\
             \x20 -i, --inline              Strip xml declarations for inline HTML use\n\
             \x20 -o, --output              Output to stdout instead of saving to a file\n\
             \x20 -dp, --decimal-precision  Round paths and numbers to 0-4 decimal places",
            args[0]
        );
        std::process::exit(1);
    }

    let input_path = &args[1];
    let input = fs::read_to_string(input_path)?;
    
    let mut config = svg_strip::StripConfig::default();
    config.inline_mode = inline_mode;
    config.decimal_precision = decimal_precision;
    
    let stripper = svg_strip::SvgStripper::with_config(config);
    let (output, stats) = stripper.strip_str(&input)?;

    if stdout_mode {
        print!("{}", output);
    } else {
        let output_path = if args.len() >= 3 {
            args[2].clone()
        } else {
            let path = Path::new(input_path);
            let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
            let parent = path.parent().unwrap_or_else(|| Path::new(""));
            let new_filename = format!("{}_stripped.svg", file_stem);
            parent.join(new_filename).to_string_lossy().into_owned()
        };

        fs::write(&output_path, &output)?;
        
        let ascii_art = " ▄▄▄▄ ▄▄ ▄▄  ▄▄▄▄      ▄▄▄▄ ▄▄▄▄▄▄ ▄▄▄▄  ▄▄ ▄▄▄▄  \n\
███▄▄ ██▄██ ██ ▄▄ ▄▄▄ ███▄▄   ██   ██▄█▄ ██ ██▄█▀ \n\
▄▄██▀  ▀█▀  ▀███▀     ▄▄██▀   ██   ██ ██ ██ ██    \n";
        
        println!("{}", ascii_art.truecolor(217, 70, 239));

        let mut summary = format!("Stripped SVG written to {}", output_path);
        if inline_mode {
            summary.push_str("\n• Inline SVG with zero overhead");
        }
        if let Some(dp) = decimal_precision {
            summary.push_str(&format!("\n• Decimal Precison for paths rounded down to {} decimals", dp));
        }
        if stats.colors_shrunk {
            summary.push_str("\n• Color Shrink to convert 6-digit hex codes to 3-digit shorthands");
        }
        println!("{}", summary);
    }
    Ok(())
}