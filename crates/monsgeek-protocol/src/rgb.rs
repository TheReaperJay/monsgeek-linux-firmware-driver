//! RGB/LED data constants.

/// Total RGB data size (126 keys * 3 bytes).
pub const TOTAL_RGB_SIZE: usize = 378;
/// Number of pages per frame.
pub const NUM_PAGES: usize = 7;
/// RGB data per full page.
pub const PAGE_SIZE: usize = 56;
/// RGB data in last page.
pub const LAST_PAGE_SIZE: usize = 42;
/// LED matrix positions (keys).
pub const MATRIX_SIZE: usize = 126;
/// Number of keys to send per chunk in streaming mode.
pub const CHUNK_SIZE: usize = 18;
/// Magic value for per-key color commands.
pub const MAGIC_VALUE: u8 = 255;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_total_rgb_size() {
        assert_eq!(TOTAL_RGB_SIZE, 378);
    }

    #[test]
    fn test_num_pages() {
        assert_eq!(NUM_PAGES, 7);
    }

    #[test]
    fn test_matrix_size() {
        assert_eq!(MATRIX_SIZE, 126);
    }
}
