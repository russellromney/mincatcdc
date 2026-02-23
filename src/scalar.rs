pub fn argmin_u32_overlapping_hashed<const SHOULD_HASH: bool>(
    bytes: &[u8],
    multiplier: u32,
    addend: u32,
) -> (usize, u32) {
    let mut min_idx = 0;
    let mut min_val = u32::MAX;

    if bytes.len() < 4 {
        return (0, u32::MAX);
    }

    for (i, window) in bytes.windows(4).enumerate() {
        let mut v = u32::from_le_bytes(window.try_into().unwrap());
        if SHOULD_HASH {
            v = v.wrapping_mul(multiplier);
            v = v.wrapping_add(addend);
        }
        if v < min_val {
            min_val = v;
            min_idx = i;
        }
    }

    (min_idx, min_val)
}

#[inline(always)]
#[allow(dead_code)]
pub fn argmin_u32_overlapping_hashed_four<const SHOULD_HASH: bool>(
    bytes: &[u8],
    multiplier: u32,
    addend: u32,
) -> (usize, u32) {
    assert!(bytes.len() >= 7);
    let mut min_idx = 0;
    let mut min_val = u32::MAX;
    
    for i in 0..4 {
        let substr = &bytes[i..i + 4];
        let mut v = u32::from_le_bytes(substr.try_into().unwrap());
        if SHOULD_HASH {
            v = v.wrapping_mul(multiplier);
            v = v.wrapping_add(addend);
        }
        if v < min_val {
            min_val = v;
            min_idx = i;
        }
    }

    (min_idx, min_val)
}
