const JAVA_RANDOM_MULTIPLIER: u64 = 0x5deece66d;
const JAVA_RANDOM_ADDEND: u64 = 0xb;
const JAVA_RANDOM_MASK: u64 = (1_u64 << 48) - 1;

/// `java.util.Random` compatible generator used by beatoraja's lane shuffle.
struct JavaRandom {
    seed: u64,
}

impl JavaRandom {
    fn new(seed: i64) -> Self {
        Self {
            seed: ((seed as u64) ^ JAVA_RANDOM_MULTIPLIER) & JAVA_RANDOM_MASK,
        }
    }

    fn next(&mut self, bits: u32) -> u32 {
        self.seed = self
            .seed
            .wrapping_mul(JAVA_RANDOM_MULTIPLIER)
            .wrapping_add(JAVA_RANDOM_ADDEND)
            & JAVA_RANDOM_MASK;
        (self.seed >> (48 - bits)) as u32
    }

    fn next_int(&mut self, bound: u32) -> u32 {
        debug_assert!(bound > 0);
        if bound.is_power_of_two() {
            return ((u64::from(bound) * u64::from(self.next(31))) >> 31) as u32;
        }

        loop {
            let bits = self.next(31);
            let value = bits % bound;
            if bits - value <= i32::MAX as u32 - (bound - 1) {
                return value;
            }
        }
    }
}

/// Reproduces beatoraja's `LaneRandomShuffleModifier` display pattern for SP 7KEY.
///
/// Each destination lane receives one source lane selected from the remaining
/// lanes with `java.util.Random(seed).nextInt(remaining)`. The scratch lane is
/// intentionally excluded, matching beatoraja's normal RANDOM option.
pub fn random_layout_for_display_7k(seed: i64) -> String {
    let mut random = JavaRandom::new(seed);
    let mut remaining = vec![1_u8, 2, 3, 4, 5, 6, 7];
    let mut layout = String::with_capacity(7);

    while !remaining.is_empty() {
        let index = random.next_int(remaining.len() as u32) as usize;
        layout.push(char::from_digit(u32::from(remaining.remove(index)), 10).unwrap_or('?'));
    }
    layout
}

/// Reproduces beatoraja's `LaneRotateShuffleModifier` display pattern for SP 7KEY.
///
/// R-RANDOM chooses a clockwise or counter-clockwise rotation and a non-identity
/// starting lane with `java.util.Random(seed)`. The result is destination-indexed,
/// matching beatoraja's result-screen `randomLayoutForDisplay` representation.
pub fn r_random_layout_for_display_7k(seed: i64) -> String {
    const KEY_COUNT: u32 = 7;

    let mut random = JavaRandom::new(seed);
    let increment = random.next_int(2) == 1;
    let mut source_lane = random.next_int(KEY_COUNT - 1) + u32::from(increment);
    let mut layout = String::with_capacity(KEY_COUNT as usize);

    for _ in 0..KEY_COUNT {
        layout.push(char::from_digit(source_lane + 1, 10).unwrap_or('?'));
        source_lane = if increment {
            (source_lane + 1) % KEY_COUNT
        } else {
            (source_lane + KEY_COUNT - 1) % KEY_COUNT
        };
    }
    layout
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_random_matches_reference_values() {
        let mut random = JavaRandom::new(0);
        assert_eq!(random.next_int(7), 5);
        assert_eq!(random.next_int(6), 4);
    }

    #[test]
    fn matches_beatoraja_lane_shuffle_vectors() {
        assert_eq!(random_layout_for_display_7k(0), "6573412");
        assert_eq!(random_layout_for_display_7k(1), "5632714");
        assert_eq!(random_layout_for_display_7k(8_005_733), "4153726");
    }

    #[test]
    fn matches_beatoraja_lane_rotate_vectors() {
        assert_eq!(r_random_layout_for_display_7k(0), "6712345");
        assert_eq!(r_random_layout_for_display_7k(1), "6712345");
        assert_eq!(r_random_layout_for_display_7k(8_005_733), "1765432");
        assert_eq!(r_random_layout_for_display_7k(-1), "6543217");
    }

    #[test]
    fn every_signed_seed_produces_a_permutation() {
        for seed in [-1, 0, i64::from(i32::MAX), i64::MAX] {
            let mut digits: Vec<char> = random_layout_for_display_7k(seed).chars().collect();
            digits.sort_unstable();
            assert_eq!(digits, ['1', '2', '3', '4', '5', '6', '7']);

            let mut rotate_digits: Vec<char> =
                r_random_layout_for_display_7k(seed).chars().collect();
            rotate_digits.sort_unstable();
            assert_eq!(rotate_digits, ['1', '2', '3', '4', '5', '6', '7']);
        }
    }
}
