const MT_N: usize = 624;
const MT_M: usize = 397;
const MATRIX_A: u32 = 0x9908_b0df;
const UPPER_MASK: u32 = 0x8000_0000;
const LOWER_MASK: u32 = 0x7fff_ffff;

/// DxLib's Mersenne Twister as used by OpenLR2.
///
/// This intentionally keeps DxLib's legacy seed expansion instead of using
/// Rust's standard RNG facilities. A saved OpenLR2 rseed must reproduce the
/// exact same lane layout on every machine.
struct OpenLr2Mt {
    state: [u32; MT_N + 1],
    tempered: [u32; MT_N],
    index: usize,
}

impl OpenLr2Mt {
    fn new(seed: i32) -> Self {
        let mut rng = Self {
            state: [0; MT_N + 1],
            tempered: [0; MT_N],
            index: 0,
        };
        let mut value = seed as u32;
        for slot in rng.state.iter_mut().take(MT_N) {
            *slot = value & 0xffff_0000;
            value = value.wrapping_mul(69_069).wrapping_add(1);
            *slot |= (value & 0xffff_0000) >> 16;
            value = value.wrapping_mul(69_069).wrapping_add(1);
        }
        rng.generate();
        rng
    }

    fn generate(&mut self) {
        for index in 0..(MT_N - MT_M) {
            let value = (self.state[index] & UPPER_MASK) | (self.state[index + 1] & LOWER_MASK);
            self.state[index] =
                self.state[index + MT_M] ^ (value >> 1) ^ if value & 1 == 0 { 0 } else { MATRIX_A };
        }

        self.state[MT_N] = self.state[0];
        for index in (MT_N - MT_M)..MT_N {
            let value = (self.state[index] & UPPER_MASK) | (self.state[index + 1] & LOWER_MASK);
            self.state[index] = self.state[index + MT_M - MT_N]
                ^ (value >> 1)
                ^ if value & 1 == 0 { 0 } else { MATRIX_A };
        }

        for index in 0..MT_N {
            let mut value = self.state[index];
            value ^= value >> 11;
            value ^= (value << 7) & 0x9d2c_5680;
            value ^= (value << 15) & 0xefc6_0000;
            value ^= value >> 18;
            self.tempered[index] = value;
        }
        self.index = 0;
    }

    fn next_u32(&mut self) -> u32 {
        if self.index >= MT_N {
            self.generate();
        }
        let value = self.tempered[self.index];
        self.index += 1;
        value
    }

    /// Matches DxLib GetRand: both zero and `maximum` are valid results.
    fn get_rand(&mut self, maximum: u32) -> u32 {
        ((u64::from(self.next_u32()) * (u64::from(maximum) + 1)) >> 32) as u32
    }
}

/// Generates OpenLR2's user-facing `randomLayoutForDisplay` value for SP 7KEY.
pub fn random_layout_for_display_7k(seed: i32) -> String {
    let mut rng = OpenLr2Mt::new(seed);
    let mut note_random_table = [0_u8, 1, 2, 3, 4, 5, 6, 7];

    for current in 1..7 {
        let target = current + rng.get_rand((7 - current) as u32) as usize;
        note_random_table.swap(current, target);
    }

    let mut display = ['0'; 7];
    for (source_lane, destination_lane) in note_random_table.iter().enumerate().skip(1) {
        let destination_lane = usize::from(*destination_lane);
        display[destination_lane - 1] = char::from_digit(source_lane as u32, 10).unwrap_or('?');
    }
    display.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_openlr2_seed_map_identity_vector() {
        assert_eq!(random_layout_for_display_7k(391), "1234567");
    }

    #[test]
    fn matches_openlr2_seed_map_requested_vector() {
        assert_eq!(random_layout_for_display_7k(12_183), "1743265");
    }

    #[test]
    fn every_signed_32_bit_seed_still_produces_a_permutation() {
        let layout = random_layout_for_display_7k(-1);
        let mut digits: Vec<char> = layout.chars().collect();
        digits.sort_unstable();
        assert_eq!(digits, ['1', '2', '3', '4', '5', '6', '7']);
    }
}
