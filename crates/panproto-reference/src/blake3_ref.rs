//! Scalar BLAKE3 derive-key hashing for the executable reference semantics.
//!
//! This is a deliberately direct implementation of the BLAKE3 specification.
//! It implements only the 32-byte derive-key mode needed by this crate.

const OUT_LEN: usize = 32;
const BLOCK_LEN: usize = 64;
const BLOCK_LEN_U32: u32 = 64;
const CHUNK_LEN: usize = 1_024;
const CHUNK_LEN_U64: u64 = 1_024;
const MAX_DEPTH: usize = 54;
const MAX_CHUNKS: u64 = 1_u64 << MAX_DEPTH;

const CHUNK_START: u32 = 1 << 0;
const CHUNK_END: u32 = 1 << 1;
const PARENT: u32 = 1 << 2;
const ROOT: u32 = 1 << 3;
const DERIVE_KEY_CONTEXT: u32 = 1 << 5;
const DERIVE_KEY_MATERIAL: u32 = 1 << 6;

const IV: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

const MESSAGE_PERMUTATION: [usize; 16] = [2, 6, 3, 10, 7, 0, 4, 13, 1, 11, 12, 5, 9, 14, 15, 8];

/// Compute the BLAKE3 derive-key output for one context and material value.
#[must_use]
pub fn derive_key(context: &str, material: &[u8]) -> [u8; OUT_LEN] {
    let mut hasher = Hasher::new_derive_key(context);
    hasher.update(material);
    hasher.finalize()
}

/// Incremental BLAKE3 derive-key hasher.
pub struct Hasher {
    key_words: [u32; 8],
    chunk_state: ChunkState,
    cv_stack: Vec<[u32; 8]>,
    flags: u32,
}

impl Hasher {
    /// Start a derive-key hash under `context`.
    #[must_use]
    pub fn new_derive_key(context: &str) -> Self {
        let context_key = hash_with_mode(IV, DERIVE_KEY_CONTEXT, context.as_bytes());
        Self::new_internal(bytes_to_words_8(&context_key), DERIVE_KEY_MATERIAL)
    }

    fn new_internal(key_words: [u32; 8], flags: u32) -> Self {
        Self {
            key_words,
            chunk_state: ChunkState::new(key_words, 0, flags),
            cv_stack: Vec::with_capacity(MAX_DEPTH),
            flags,
        }
    }

    /// Add material bytes to this hash.
    pub fn update(&mut self, mut input: &[u8]) -> &mut Self {
        let current_len = self.chunk_state.chunk_counter * CHUNK_LEN_U64
            + u64::try_from(self.chunk_state.len()).unwrap_or_else(|_| unreachable!());
        let input_len = u64::try_from(input.len())
            .unwrap_or_else(|_| panic!("BLAKE3 input length must be less than 2^64 bytes"));
        assert!(
            input_len <= u64::MAX - current_len,
            "BLAKE3 input length must be less than 2^64 bytes"
        );
        while !input.is_empty() {
            if self.chunk_state.len() == CHUNK_LEN {
                assert!(
                    self.chunk_state.chunk_counter < MAX_CHUNKS - 1,
                    "BLAKE3 input length must be less than 2^64 bytes"
                );
                let chunk_cv = self.chunk_state.output().chaining_value();
                let total_chunks = self.chunk_state.chunk_counter + 1;
                self.push_chunk_cv(chunk_cv, total_chunks);
                self.chunk_state = ChunkState::new(self.key_words, total_chunks, self.flags);
            }

            let take = input.len().min(CHUNK_LEN - self.chunk_state.len());
            let (for_chunk, rest) = input.split_at(take);
            self.chunk_state.update(for_chunk);
            input = rest;
        }
        self
    }

    /// Finish this hash without changing its state.
    #[must_use]
    pub fn finalize(&self) -> [u8; OUT_LEN] {
        let mut output = self.chunk_state.output();
        for left_cv in self.cv_stack.iter().rev() {
            output = parent_output(
                *left_cv,
                output.chaining_value(),
                self.key_words,
                self.flags,
            );
        }
        output.root_hash()
    }

    fn push_chunk_cv(&mut self, mut cv: [u32; 8], mut total_chunks: u64) {
        while total_chunks & 1 == 0 {
            let Some(left_cv) = self.cv_stack.pop() else {
                unreachable!("a completed right subtree must have a left sibling");
            };
            cv = parent_output(left_cv, cv, self.key_words, self.flags).chaining_value();
            total_chunks >>= 1;
        }
        assert!(
            self.cv_stack.len() < MAX_DEPTH,
            "BLAKE3 input length must be less than 2^64 bytes"
        );
        self.cv_stack.push(cv);
    }
}

struct ChunkState {
    chaining_value: [u32; 8],
    chunk_counter: u64,
    block: [u8; BLOCK_LEN],
    block_len: usize,
    blocks_compressed: usize,
    flags: u32,
}

impl ChunkState {
    const fn new(key_words: [u32; 8], chunk_counter: u64, flags: u32) -> Self {
        Self {
            chaining_value: key_words,
            chunk_counter,
            block: [0; BLOCK_LEN],
            block_len: 0,
            blocks_compressed: 0,
            flags,
        }
    }

    const fn len(&self) -> usize {
        self.blocks_compressed * BLOCK_LEN + self.block_len
    }

    const fn start_flag(&self) -> u32 {
        if self.blocks_compressed == 0 {
            CHUNK_START
        } else {
            0
        }
    }

    fn update(&mut self, mut input: &[u8]) {
        while !input.is_empty() {
            if self.block_len == BLOCK_LEN {
                let block_words = bytes_to_words_16(&self.block);
                let words = compress(
                    self.chaining_value,
                    block_words,
                    self.chunk_counter,
                    BLOCK_LEN_U32,
                    self.flags | self.start_flag(),
                );
                self.chaining_value.copy_from_slice(&words[..8]);
                self.blocks_compressed += 1;
                self.block = [0; BLOCK_LEN];
                self.block_len = 0;
            }

            let take = input.len().min(BLOCK_LEN - self.block_len);
            let (for_block, rest) = input.split_at(take);
            self.block[self.block_len..self.block_len + take].copy_from_slice(for_block);
            self.block_len += take;
            input = rest;
        }
    }

    fn output(&self) -> Output {
        Output {
            input_chaining_value: self.chaining_value,
            block_words: bytes_to_words_16(&self.block),
            counter: self.chunk_counter,
            block_len: u32::try_from(self.block_len).unwrap_or_else(|_| unreachable!()),
            flags: self.flags | self.start_flag() | CHUNK_END,
        }
    }
}

struct Output {
    input_chaining_value: [u32; 8],
    block_words: [u32; 16],
    counter: u64,
    block_len: u32,
    flags: u32,
}

impl Output {
    fn chaining_value(&self) -> [u32; 8] {
        let words = compress(
            self.input_chaining_value,
            self.block_words,
            self.counter,
            self.block_len,
            self.flags,
        );
        let mut cv = [0; 8];
        cv.copy_from_slice(&words[..8]);
        cv
    }

    fn root_hash(&self) -> [u8; OUT_LEN] {
        let words = compress(
            self.input_chaining_value,
            self.block_words,
            0,
            self.block_len,
            self.flags | ROOT,
        );
        words_8_to_bytes(words[..8].try_into().unwrap_or_else(|_| unreachable!()))
    }
}

fn parent_output(left_cv: [u32; 8], right_cv: [u32; 8], key_words: [u32; 8], flags: u32) -> Output {
    let mut block_words = [0; 16];
    block_words[..8].copy_from_slice(&left_cv);
    block_words[8..].copy_from_slice(&right_cv);
    Output {
        input_chaining_value: key_words,
        block_words,
        counter: 0,
        block_len: BLOCK_LEN_U32,
        flags: flags | PARENT,
    }
}

fn hash_with_mode(key_words: [u32; 8], flags: u32, input: &[u8]) -> [u8; OUT_LEN] {
    let mut hasher = Hasher::new_internal(key_words, flags);
    hasher.update(input);
    hasher.finalize()
}

fn compress(
    chaining_value: [u32; 8],
    block_words: [u32; 16],
    counter: u64,
    block_len: u32,
    flags: u32,
) -> [u32; 16] {
    let counter_bytes = counter.to_le_bytes();
    let counter_low = u32::from_le_bytes(
        counter_bytes[..4]
            .try_into()
            .unwrap_or_else(|_| unreachable!()),
    );
    let counter_high = u32::from_le_bytes(
        counter_bytes[4..]
            .try_into()
            .unwrap_or_else(|_| unreachable!()),
    );
    let mut state = [
        chaining_value[0],
        chaining_value[1],
        chaining_value[2],
        chaining_value[3],
        chaining_value[4],
        chaining_value[5],
        chaining_value[6],
        chaining_value[7],
        IV[0],
        IV[1],
        IV[2],
        IV[3],
        counter_low,
        counter_high,
        block_len,
        flags,
    ];
    let mut message = block_words;
    for round_index in 0..7 {
        round(&mut state, message);
        if round_index != 6 {
            message = permute(message);
        }
    }
    for index in 0..8 {
        state[index] ^= state[index + 8];
        state[index + 8] ^= chaining_value[index];
    }
    state
}

const fn round(state: &mut [u32; 16], message: [u32; 16]) {
    mix(state, 0, 4, 8, 12, message[0], message[1]);
    mix(state, 1, 5, 9, 13, message[2], message[3]);
    mix(state, 2, 6, 10, 14, message[4], message[5]);
    mix(state, 3, 7, 11, 15, message[6], message[7]);
    mix(state, 0, 5, 10, 15, message[8], message[9]);
    mix(state, 1, 6, 11, 12, message[10], message[11]);
    mix(state, 2, 7, 8, 13, message[12], message[13]);
    mix(state, 3, 4, 9, 14, message[14], message[15]);
}

const fn mix(
    state: &mut [u32; 16],
    index_a: usize,
    index_b: usize,
    index_c: usize,
    index_d: usize,
    message_x: u32,
    message_y: u32,
) {
    state[index_a] = state[index_a]
        .wrapping_add(state[index_b])
        .wrapping_add(message_x);
    state[index_d] = (state[index_d] ^ state[index_a]).rotate_right(16);
    state[index_c] = state[index_c].wrapping_add(state[index_d]);
    state[index_b] = (state[index_b] ^ state[index_c]).rotate_right(12);
    state[index_a] = state[index_a]
        .wrapping_add(state[index_b])
        .wrapping_add(message_y);
    state[index_d] = (state[index_d] ^ state[index_a]).rotate_right(8);
    state[index_c] = state[index_c].wrapping_add(state[index_d]);
    state[index_b] = (state[index_b] ^ state[index_c]).rotate_right(7);
}

fn permute(message: [u32; 16]) -> [u32; 16] {
    let mut permuted = [0; 16];
    for (index, source) in MESSAGE_PERMUTATION.into_iter().enumerate() {
        permuted[index] = message[source];
    }
    permuted
}

fn bytes_to_words_8(bytes: &[u8; OUT_LEN]) -> [u32; 8] {
    let mut words = [0; 8];
    for (word, bytes) in words.iter_mut().zip(bytes.chunks_exact(4)) {
        *word = u32::from_le_bytes(bytes.try_into().unwrap_or_else(|_| unreachable!()));
    }
    words
}

fn bytes_to_words_16(bytes: &[u8; BLOCK_LEN]) -> [u32; 16] {
    let mut words = [0; 16];
    for (word, bytes) in words.iter_mut().zip(bytes.chunks_exact(4)) {
        *word = u32::from_le_bytes(bytes.try_into().unwrap_or_else(|_| unreachable!()));
    }
    words
}

fn words_8_to_bytes(words: [u32; 8]) -> [u8; OUT_LEN] {
    let mut bytes = [0; OUT_LEN];
    for (word, output) in words.into_iter().zip(bytes.chunks_exact_mut(4)) {
        output.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::{BLOCK_LEN, Hasher, MAX_CHUNKS, derive_key};

    const OFFICIAL_CONTEXT: &str = "BLAKE3 2019-12-27 16:29:52 test vectors context";
    // First 32 bytes of every official derive-key vector at
    // https://raw.githubusercontent.com/BLAKE3-team/BLAKE3/
    // f55849f89cd85083c1c9daa2c5de20766f309c00/test_vectors/test_vectors.json.
    // That file is offered under CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH
    // LLVM-exception. Its input repeats bytes 0 through 250, and its context
    // is OFFICIAL_CONTEXT. These constants are test data, not implementation
    // code; this module's implementation was written directly from the spec.
    const OFFICIAL_VECTORS: &[(usize, &str)] = &[
        (
            0,
            "2cc39783c223154fea8dfb7c1b1660f2ac2dcbd1c1de8277b0b0dd39b7e50d7d",
        ),
        (
            1,
            "b3e2e340a117a499c6cf2398a19ee0d29cca2bb7404c73063382693bf66cb06c",
        ),
        (
            2,
            "1f166565a7df0098ee65922d7fea425fb18b9943f19d6161e2d17939356168e6",
        ),
        (
            3,
            "440aba35cb006b61fc17c0529255de438efc06a8c9ebf3f2ddac3b5a86705797",
        ),
        (
            4,
            "f46085c8190d69022369ce1a18880e9b369c135eb93f3c63550d3e7630e91060",
        ),
        (
            5,
            "1f24eda69dbcb752847ec3ebb5dd42836d86e58500c7c98d906ecd82ed9ae47f",
        ),
        (
            6,
            "be96b30b37919fe4379dfbe752ae77b4f7e2ab92f7ff27435f76f2f065f6a5f4",
        ),
        (
            7,
            "dc3b6485f9d94935329442916b0d059685ba815a1fa2a14107217453a7fc9f0e",
        ),
        (
            8,
            "2b166978cef14d9d438046c720519d8b1cad707e199746f1562d0c87fbd32940",
        ),
        (
            63,
            "b6451e30b953c206e34644c6803724e9d2725e0893039cfc49584f991f451af3",
        ),
        (
            64,
            "a5c4a7053fa86b64746d4bb688d06ad1f02a18fce9afd3e818fefaa7126bf73e",
        ),
        (
            65,
            "51fd05c3c1cfbc8ed67d139ad76f5cf8236cd2acd26627a30c104dfd9d3ff8a8",
        ),
        (
            127,
            "c91c090ceee3a3ac81902da31838012625bbcd73fcb92e7d7e56f78deba4f0c3",
        ),
        (
            128,
            "81720f34452f58a0120a58b6b4608384b5c51d11f39ce97161a0c0e442ca0225",
        ),
        (
            129,
            "938d2d4435be30eafdbb2b7031f7857c98b04881227391dc40db3c7b21f41fc1",
        ),
        (
            1_023,
            "74a16c1c3d44368a86e1ca6df64be6a2f64cce8f09220787450722d85725dea5",
        ),
        (
            1_024,
            "7356cd7720d5b66b6d0697eb3177d9f8d73a4a5c5e968896eb6a689684302706",
        ),
        (
            1_025,
            "effaa245f065fbf82ac186839a249707c3bddf6d3fdda22d1b95a3c970379bcb",
        ),
        (
            2_048,
            "7b2945cb4fef70885cc5d78a87bf6f6207dd901ff239201351ffac04e1088a23",
        ),
        (
            2_049,
            "2ea477c5515cc3dd606512ee72bb3e0e758cfae7232826f35fb98ca1bcbdf273",
        ),
        (
            3_072,
            "050df97f8c2ead654d9bb3ab8c9178edcd902a32f8495949feadcc1e0480c46b",
        ),
        (
            3_073,
            "72613c9ec9ff7e40f8f5c173784c532ad852e827dba2bf85b2ab4b76f7079081",
        ),
        (
            4_096,
            "1e0d7f3db8c414c97c6307cbda6cd27ac3b030949da8e23be1a1a924ad2f25b9",
        ),
        (
            4_097,
            "aca51029626b55fda7117b42a7c211f8c6e9ba4fe5b7a8ca922f34299500ead8",
        ),
        (
            5_120,
            "7a7acac8a02adcf3038d74cdd1d34527de8a0fcc0ee3399d1262397ce5817f60",
        ),
        (
            5_121,
            "b07f01e518e702f7ccb44a267e9e112d403a7b3f4883a47ffbed4b48339b3c34",
        ),
        (
            6_144,
            "2a95beae63ddce523762355cf4b9c1d8f131465780a391286a5d01abb5683a15",
        ),
        (
            6_145,
            "379bcc61d0051dd489f686c13de00d5b14c505245103dc040d9e4dd1facab8e5",
        ),
        (
            7_168,
            "11c37a112765370c94a51415d0d651190c288566e295d505defdad895dae2237",
        ),
        (
            7_169,
            "554b0a5efea9ef183f2f9b931b7497995d9eb26f5c5c6dad2b97d62fc5ac31d9",
        ),
        (
            8_192,
            "ad01d7ae4ad059b0d33baa3c01319dcf8088094d0359e5fd45d6aeaa8b2d0c3d",
        ),
        (
            8_193,
            "af1e0346e389b17c23200270a64aa4e1ead98c61695d917de7d5b00491c9b0f1",
        ),
        (
            16_384,
            "160e18b5878cd0df1c3af85eb25a0db5344d43a6fbd7a8ef4ed98d0714c3f7e1",
        ),
        (
            31_744,
            "39772aef80e0ebe60596361e45b061e8f417429d529171b6764468c22928e28e",
        ),
        (
            102_400,
            "4652cff7a3f385a6103b5c260fc1593e13c778dbe608efb092fe7ee69df6e9c6",
        ),
    ];

    fn official_input(length: usize) -> Vec<u8> {
        (0..length)
            .map(|index| u8::try_from(index % 251).unwrap_or_else(|_| unreachable!()))
            .collect()
    }

    fn decode_digest(hex: &str) -> [u8; 32] {
        assert_eq!(hex.len(), 64, "a digest must contain 64 hexadecimal digits");
        let mut digest = [0; 32];
        for (index, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
            digest[index] = (hex_nibble(pair[0]) << 4) | hex_nibble(pair[1]);
        }
        digest
    }

    fn hex_nibble(byte: u8) -> u8 {
        match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            _ => panic!("official vectors contain only lowercase hexadecimal digits"),
        }
    }

    #[test]
    fn all_official_derive_key_vectors_match() {
        for &(length, expected) in OFFICIAL_VECTORS {
            let input = official_input(length);
            assert_eq!(
                derive_key(OFFICIAL_CONTEXT, &input),
                decode_digest(expected),
                "official derive-key vector length {length}"
            );
        }
    }

    #[test]
    fn streaming_partitions_match_official_vectors() {
        for &(length, expected) in OFFICIAL_VECTORS {
            let input = official_input(length);
            let expected = decode_digest(expected);
            let mut hasher = Hasher::new_derive_key(OFFICIAL_CONTEXT);
            for chunk in input.chunks(1) {
                hasher.update(chunk);
            }
            assert_eq!(
                hasher.finalize(),
                expected,
                "byte partitions at length {length}"
            );

            let mut hasher = Hasher::new_derive_key(OFFICIAL_CONTEXT);
            let mut offset = 0;
            let widths = [1, 63, 64, 65, 1_023, 1_024];
            for width in widths.into_iter().cycle() {
                if offset >= input.len() {
                    break;
                }
                let end = (offset + width).min(input.len());
                hasher.update(&input[offset..end]);
                offset = end;
            }
            assert_eq!(
                hasher.finalize(),
                expected,
                "irregular partitions at length {length}"
            );
        }
    }

    #[test]
    fn every_boundary_split_matches_official_vectors() {
        for length in [63, 64, 65, 1_023, 1_024, 1_025, 2_048, 2_049, 4_096, 4_097] {
            let input = official_input(length);
            let expected = OFFICIAL_VECTORS
                .iter()
                .find_map(|&(candidate, digest)| {
                    (candidate == length).then(|| decode_digest(digest))
                })
                .unwrap_or_else(|| unreachable!("every selected length has an official vector"));
            for split in 0..=length {
                let mut hasher = Hasher::new_derive_key(OFFICIAL_CONTEXT);
                hasher.update(&input[..split]);
                hasher.update(&input[split..]);
                assert_eq!(
                    hasher.finalize(),
                    expected,
                    "length {length}, split {split}"
                );
            }
        }
    }

    #[test]
    fn graph_identity_partition_and_nonmutating_finalize_are_stable() {
        let input = official_input(4_097);
        let expected =
            decode_digest("aca51029626b55fda7117b42a7c211f8c6e9ba4fe5b7a8ca922f34299500ead8");
        let mut hasher = Hasher::new_derive_key(OFFICIAL_CONTEXT);
        hasher.update(&input[..32]);
        hasher.update(&[]);
        hasher.update(&input[32..64]);
        hasher.update(&input[64..]);
        assert_eq!(hasher.finalize(), expected, "32 + 32 + remainder partition");
        assert_eq!(
            hasher.finalize(),
            expected,
            "finalize must not change state"
        );
    }

    #[test]
    fn long_utf8_context_matches_pinned_external_cross_check() {
        let context = format!("{}lambda-\u{03bb}", "context-".repeat(160));
        let material = official_input(2_049);
        assert_eq!(
            context.len(),
            1_289,
            "the context must cross a chunk boundary"
        );
        assert_eq!(
            derive_key(&context, &material),
            decode_digest("a15e7f9628e9f8abe001728813a0c684e1783c9285c09bff40bdadc40bf0ef5b")
        );
    }

    #[test]
    fn maximum_valid_input_length_accepts_no_more_bytes() {
        let mut hasher = Hasher::new_derive_key(OFFICIAL_CONTEXT);
        hasher.chunk_state.chunk_counter = MAX_CHUNKS - 1;
        hasher.chunk_state.blocks_compressed = 15;
        hasher.chunk_state.block_len = BLOCK_LEN - 2;
        hasher.update(&[0]);
        hasher.update(&[]);
        let first = hasher.finalize();
        assert_eq!(
            hasher.finalize(),
            first,
            "maximum valid length remains finalizable"
        );
    }

    #[test]
    #[should_panic(expected = "BLAKE3 input length must be less than 2^64 bytes")]
    fn input_length_two_to_the_64_is_rejected_before_filling_the_block() {
        let mut hasher = Hasher::new_derive_key(OFFICIAL_CONTEXT);
        hasher.chunk_state.chunk_counter = MAX_CHUNKS - 1;
        hasher.chunk_state.blocks_compressed = 15;
        hasher.chunk_state.block_len = BLOCK_LEN - 1;
        hasher.update(&[0]);
    }
}
