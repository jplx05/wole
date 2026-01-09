use anyhow::{bail, Result};

/// Parse human-readable size strings to bytes
/// 
/// Supports: B, KB, MB, GB, TB (case-insensitive)
/// Examples:
/// - "100MB" -> 104_857_600
/// - "1GB"   -> 1_073_741_824
/// - "500KB" -> 512_000
pub fn parse_size(s: &str) -> Result<u64> {
    let s = s.trim();
    
    if s.is_empty() {
        bail!("Empty size string");
    }
    
    // Find where the number ends and unit begins
    let mut num_end = s.len();
    for (i, c) in s.char_indices() {
        if !c.is_ascii_digit() && c != '.' {
            num_end = i;
            break;
        }
    }
    
    if num_end == s.len() {
        // No unit found, assume bytes
        return Ok(s.parse::<u64>()?);
    }
    
    if num_end == 0 {
        // String starts with non-digit, invalid format
        bail!("Size string must start with a number: {}", s);
    }
    
    let num_str = &s[..num_end];
    let unit_str = s[num_end..].trim().to_uppercase();
    
    let num: f64 = num_str.parse()
        .map_err(|_| anyhow::anyhow!("Invalid number: {}", num_str))?;
    
    let multiplier = match unit_str.as_str() {
        "B" => 1u64,
        "KB" => 1024u64,
        "MB" => 1024u64 * 1024,
        "GB" => 1024u64 * 1024 * 1024,
        "TB" => 1024u64 * 1024 * 1024 * 1024,
        _ => bail!("Unknown size unit: {}. Supported: B, KB, MB, GB, TB", unit_str),
    };
    
    let bytes = (num * multiplier as f64) as u64;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("100MB").unwrap(), 104_857_600);
        assert_eq!(parse_size("1GB").unwrap(), 1_073_741_824);
        assert_eq!(parse_size("500KB").unwrap(), 512_000);
        assert_eq!(parse_size("1024B").unwrap(), 1024);
        assert_eq!(parse_size("2TB").unwrap(), 2_199_023_255_552);
        assert_eq!(parse_size("100").unwrap(), 100); // No unit = bytes
        assert_eq!(parse_size("1.5GB").unwrap(), 1_610_612_736);
    }
    
    #[test]
    fn test_case_insensitive() {
        assert_eq!(parse_size("100mb").unwrap(), parse_size("100MB").unwrap());
        assert_eq!(parse_size("1gb").unwrap(), parse_size("1GB").unwrap());
    }
    
    #[test]
    fn test_parse_size_errors() {
        assert!(parse_size("").is_err());
        assert!(parse_size("abc").is_err());
        assert!(parse_size("MB").is_err()); // No number
    }
    
    #[test]
    fn test_parse_size_decimal() {
        assert_eq!(parse_size("0.5GB").unwrap(), 536_870_912);
        assert_eq!(parse_size("2.5MB").unwrap(), 2_621_440);
    }
}
