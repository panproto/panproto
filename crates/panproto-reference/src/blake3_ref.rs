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

const CHUNK_START: u32 = 1;
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
        if input.is_empty() {
            return self;
        }

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
            assert_ne!(take, 0, "BLAKE3 chunk update must make progress");
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
                self.chaining_value = compress(
                    self.chaining_value,
                    block_words,
                    self.chunk_counter,
                    BLOCK_LEN_U32,
                    self.flags + self.start_flag(),
                );
                self.blocks_compressed += 1;
                self.block = [0; BLOCK_LEN];
                self.block_len = 0;
            }

            let take = input.len().min(BLOCK_LEN - self.block_len);
            assert_ne!(take, 0, "BLAKE3 block update must make progress");
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
            flags: self.flags + self.start_flag() + CHUNK_END,
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
        compress(
            self.input_chaining_value,
            self.block_words,
            self.counter,
            self.block_len,
            self.flags,
        )
    }

    fn root_hash(&self) -> [u8; OUT_LEN] {
        words_8_to_bytes(compress(
            self.input_chaining_value,
            self.block_words,
            0,
            self.block_len,
            self.flags + ROOT,
        ))
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
        flags: flags + PARENT,
    }
}

fn hash_with_mode(key_words: [u32; 8], flags: u32, input: &[u8]) -> [u8; OUT_LEN] {
    let mut hasher = Hasher::new_internal(key_words, flags);
    hasher.update(input);
    hasher.finalize()
}

/// The compression function, truncated to the eight words of a chaining
/// value or a 32-byte output. The specification's other eight output words
/// (`state[8..]` fed forward with the input chaining value) only extend the
/// output past 32 bytes, which this module never produces.
fn compress(
    chaining_value: [u32; 8],
    block_words: [u32; 16],
    counter: u64,
    block_len: u32,
    flags: u32,
) -> [u32; 8] {
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
    std::array::from_fn(|index| state[index] ^ state[index + 8])
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
    use super::{BLOCK_LEN, Hasher, IV, MAX_CHUNKS, bytes_to_words_8, derive_key, hash_with_mode};

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

    // The `hash` and `keyed_hash` vectors of the same file, first 32 bytes
    // each, for every listed input length. The fetched file's SHA-256 is
    // dcb91ea8accc77e6d6e632af7cdc1a99a9f3ae78cf648da595c7d064db32f624.
    const OFFICIAL_HASH_AND_KEYED_HASH_VECTORS: &[(usize, &str, &str)] = &[
        (
            0,
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262",
            "92b2b75604ed3c761f9d6f62392c8a9227ad0ea3f09573e783f1498a4ed60d26",
        ),
        (
            1,
            "2d3adedff11b61f14c886e35afa036736dcd87a74d27b5c1510225d0f592e213",
            "6d7878dfff2f485635d39013278ae14f1454b8c0a3a2d34bc1ab38228a80c95b",
        ),
        (
            2,
            "7b7015bb92cf0b318037702a6cdd81dee41224f734684c2c122cd6359cb1ee63",
            "5392ddae0e0a69d5f40160462cbd9bd889375082ff224ac9c758802b7a6fd20a",
        ),
        (
            3,
            "e1be4d7a8ab5560aa4199eea339849ba8e293d55ca0a81006726d184519e647f",
            "39e67b76b5a007d4921969779fe666da67b5213b096084ab674742f0d5ec62b9",
        ),
        (
            4,
            "f30f5ab28fe047904037f77b6da4fea1e27241c5d132638d8bedce9d40494f32",
            "7671dde590c95d5ac9616651ff5aa0a27bee5913a348e053b8aa9108917fe070",
        ),
        (
            5,
            "b40b44dfd97e7a84a996a91af8b85188c66c126940ba7aad2e7ae6b385402aa2",
            "73ac69eecf286894d8102018a6fc729f4b1f4247d3703f69bdc6a5fe3e0c8461",
        ),
        (
            6,
            "06c4e8ffb6872fad96f9aaca5eee1553eb62aed0ad7198cef42e87f6a616c844",
            "82d3199d0013035682cc7f2a399d4c212544376a839aa863a0f4c91220ca7a6d",
        ),
        (
            7,
            "3f8770f387faad08faa9d8414e9f449ac68e6ff0417f673f602a646a891419fe",
            "af0a7ec382aedc0cfd626e49e7628bc7a353a4cb108855541a5651bf64fbb28a",
        ),
        (
            8,
            "2351207d04fc16ade43ccab08600939c7c1fa70a5c0aaca76063d04c3228eaeb",
            "be2f5495c61cba1bb348a34948c004045e3bd4dae8f0fe82bf44d0da245a0600",
        ),
        (
            63,
            "e9bc37a594daad83be9470df7f7b3798297c3d834ce80ba85d6e207627b7db7b",
            "bb1eb5d4afa793c1ebdd9fb08def6c36d10096986ae0cfe148cd101170ce37ae",
        ),
        (
            64,
            "4eed7141ea4a5cd4b788606bd23f46e212af9cacebacdc7d1f4c6dc7f2511b98",
            "ba8ced36f327700d213f120b1a207a3b8c04330528586f414d09f2f7d9ccb7e6",
        ),
        (
            65,
            "de1e5fa0be70df6d2be8fffd0e99ceaa8eb6e8c93a63f2d8d1c30ecb6b263dee",
            "c0a4edefa2d2accb9277c371ac12fcdbb52988a86edc54f0716e1591b4326e72",
        ),
        (
            127,
            "d81293fda863f008c09e92fc382a81f5a0b4a1251cba1634016a0f86a6bd640d",
            "c64200ae7dfaf35577ac5a9521c47863fb71514a3bcad18819218b818de85818",
        ),
        (
            128,
            "f17e570564b26578c33bb7f44643f539624b05df1a76c81f30acd548c44b45ef",
            "b04fe15577457267ff3b6f3c947d93be581e7e3a4b018679125eaf86f6a628ec",
        ),
        (
            129,
            "683aaae9f3c5ba37eaaf072aed0f9e30bac0865137bae68b1fde4ca2aebdcb12",
            "d4a64dae6cdccbac1e5287f54f17c5f985105457c1a2ec1878ebd4b57e20d38f",
        ),
        (
            1_023,
            "10108970eeda3eb932baac1428c7a2163b0e924c9a9e25b35bba72b28f70bd11",
            "c951ecdf03288d0fcc96ee3413563d8a6d3589547f2c2fb36d9786470f1b9d6e",
        ),
        (
            1_024,
            "42214739f095a406f3fc83deb889744ac00df831c10daa55189b5d121c855af7",
            "75c46f6f3d9eb4f55ecaaee480db732e6c2105546f1e675003687c31719c7ba4",
        ),
        (
            1_025,
            "d00278ae47eb27b34faecf67b4fe263f82d5412916c1ffd97c8cb7fb814b8444",
            "357dc55de0c7e382c900fd6e320acc04146be01db6a8ce7210b7189bd664ea69",
        ),
        (
            2_048,
            "e776b6028c7cd22a4d0ba182a8bf62205d2ef576467e838ed6f2529b85fba24a",
            "879cf1fa2ea0e79126cb1063617a05b6ad9d0b696d0d757cf053439f60a99dd1",
        ),
        (
            2_049,
            "5f4d72f40d7a5f82b15ca2b2e44b1de3c2ef86c426c95c1af0b6879522563030",
            "9f29700902f7c86e514ddc4df1e3049f258b2472b6dd5267f61bf13983b78dd5",
        ),
        (
            3_072,
            "b98cb0ff3623be03326b373de6b9095218513e64f1ee2edd2525c7ad1e5cffd2",
            "044a0e7b172a312dc02a4c9a818c036ffa2776368d7f528268d2e6b5df191770",
        ),
        (
            3_073,
            "7124b49501012f81cc7f11ca069ec9226cecb8a2c850cfe644e327d22d3e1cd3",
            "68dede9bef00ba89e43f31a6825f4cf433389fedae75c04ee9f0cf16a427c95a",
        ),
        (
            4_096,
            "015094013f57a5277b59d8475c0501042c0b642e531b0a1c8f58d2163229e969",
            "befc660aea2f1718884cd8deb9902811d332f4fc4a38cf7c7300d597a081bfc0",
        ),
        (
            4_097,
            "9b4052b38f1c5fc8b1f9ff7ac7b27cd242487b3d890d15c96a1c25b8aa0fb995",
            "00df940cd36bb9fa7cbbc3556744e0dbc8191401afe70520ba292ee3ca80abbc",
        ),
        (
            5_120,
            "9cadc15fed8b5d854562b26a9536d9707cadeda9b143978f319ab34230535833",
            "2c493e48e9b9bf31e0553a22b23503c0a3388f035cece68eb438d22fa1943e20",
        ),
        (
            5_121,
            "628bd2cb2004694adaab7bbd778a25df25c47b9d4155a55f8fbd79f2fe154cff",
            "6ccf1c34753e7a044db80798ecd0782a8f76f33563accaddbfbb2e0ea4b2d024",
        ),
        (
            6_144,
            "3e2e5b74e048f3add6d21faab3f83aa44d3b2278afb83b80b3c35164ebeca205",
            "3d6b6d21281d0ade5b2b016ae4034c5dec10ca7e475f90f76eac7138e9bc8f1d",
        ),
        (
            6_145,
            "f1323a8631446cc50536a9f705ee5cb619424d46887f3c376c695b70e0f0507f",
            "9ac301e9e39e45e3250a7e3b3df701aa0fb6889fbd80eeecf28dbc6300fbc539",
        ),
        (
            7_168,
            "61da957ec2499a95d6b8023e2b0e604ec7f6b50e80a9678b89d2628e99ada77a",
            "b42835e40e9d4a7f42ad8cc04f85a963a76e18198377ed84adddeaecacc6f3fc",
        ),
        (
            7_169,
            "a003fc7a51754a9b3c7fae0367ab3d782dccf28855a03d435f8cfe74605e7817",
            "ed9b1a922c046fdb3d423ae34e143b05ca1bf28b710432857bf738bcedbfa511",
        ),
        (
            8_192,
            "aae792484c8efe4f19e2ca7d371d8c467ffb10748d8a5a1ae579948f718a2a63",
            "dc9637c8845a770b4cbf76b8daec0eebf7dc2eac11498517f08d44c8fc00d58a",
        ),
        (
            8_193,
            "bab6c09cb8ce8cf459261398d2e7aef35700bf488116ceb94a36d0f5f1b7bc3b",
            "954a2a75420c8d6547e3ba5b98d963e6fa6491addc8c023189cc519821b4a1f5",
        ),
        (
            16_384,
            "f875d6646de28985646f34ee13be9a576fd515f76b5b0a26bb324735041ddde4",
            "9e9fc4eb7cf081ea7c47d1807790ed211bfec56aa25bb7037784c13c4b707b0d",
        ),
        (
            31_744,
            "62b6960e1a44bcc1eb1a611a8d6235b6b4b78f32e7abc4fb4c6cdcce94895c47",
            "efa53b389ab67c593dba624d898d0f7353ab99e4ac9d42302ee64cbf9939a419",
        ),
        (
            102_400,
            "bc3e3d41a1146b069abffad3c0d44860cf664390afce4d9661f7902e7943e085",
            "1c35d1a5811083fd7119f5d5d1ba027b4d01c0c6c49fb6ff2cf75393ea5db4a7",
        ),
    ];

    /// The keyed-hash mode flag and the vectors' key, from the specification
    /// and the vector file.
    const KEYED_HASH: u32 = 1 << 4;
    const OFFICIAL_KEY: &[u8; 32] = b"whats the Elvish word for friend";

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
    fn all_official_hash_and_keyed_hash_vectors_match() {
        let key_words = bytes_to_words_8(OFFICIAL_KEY);
        for &(length, hash, keyed_hash) in OFFICIAL_HASH_AND_KEYED_HASH_VECTORS {
            let input = official_input(length);
            assert_eq!(
                hash_with_mode(IV, 0, &input),
                decode_digest(hash),
                "official hash vector length {length}"
            );
            assert_eq!(
                hash_with_mode(key_words, KEYED_HASH, &input),
                decode_digest(keyed_hash),
                "official keyed-hash vector length {length}"
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
    fn empty_update_is_the_streaming_identity() {
        let prefix = official_input(1_023);
        let suffix = official_input(66);
        let mut material = prefix.clone();
        material.extend_from_slice(&suffix);

        let mut hasher = Hasher::new_derive_key(OFFICIAL_CONTEXT);
        hasher.update(&prefix);
        let prefix_digest = hasher.finalize();
        hasher.update(&[]);
        assert_eq!(hasher.finalize(), prefix_digest);
        hasher.update(&suffix);

        assert_eq!(hasher.finalize(), derive_key(OFFICIAL_CONTEXT, &material));
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
