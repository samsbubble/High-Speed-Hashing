#![feature(test, sip_hash_13)]

extern crate byteorder;
extern crate test;

use std::env;
use std::fs::File;
use std::io::Read;
use std::process;
use std::time::Instant;

#[allow(deprecated)]
use std::hash::{Hasher, SipHasher};

use byteorder::{BigEndian, ByteOrder};

pub mod imp;

////////////////////////////////////////
// Entry Point
////////////////////////////////////////

#[derive(Clone, Copy, Debug)]
pub enum OutputMode {
    Csv,
    Pretty,
}

impl OutputMode {
    pub fn is_csv(self) -> bool {
        match self {
            OutputMode::Csv => true,
            _ => false,
        }
    }

    pub fn is_pretty(self) -> bool {
        match self {
            OutputMode::Pretty => true,
            _ => true,
        }
    }
}

pub fn main() {
    // Read input file.

    let mut input_raw = Vec::new();
    {
        let mut file = File::open("input.txt").unwrap();
        file.read_to_end(&mut input_raw).unwrap();
    }

    // Parse arguments.

    let mut mode = None;
    let mut experiment = 0;

    let mut args = env::args();

    let argv0 = args.next().unwrap();

    for arg in args {
        match &arg[..] {
            "-c" => mode = Some(OutputMode::Csv),
            "-p" => mode = Some(OutputMode::Pretty),
            "1" => experiment = 1,
            "2" => experiment = 2,
            _ => {
                eprintln!("Usage: cargo run --release -- [-cp] 1-2");
                eprintln!("{}: unexpected argument {:?}", argv0, arg);
                process::exit(2);
            }
        }
    }

    // Validate arguments.

    if experiment == 0 {
        mode = None;
    }

    let mode = match mode {
        Some(mode) => mode,
        None => {
            eprintln!("Usage: cargo run --release -- [-cp] 1-2");
            eprintln!("Options:");
            eprintln!("  -c        Output as CSV.");
            eprintln!("  -p        Output as human-readable text.");
            eprintln!("Arguments:");
            eprintln!("  1-2       Select the experiment to perform.");
            process::exit(2);
        }
    };

    // Perform experiment.

    match experiment {
        1 => experiment_1(mode, &input_raw),
        2 => experiment_2(mode, &input_raw),
        _ => unreachable!(),
    }
}

////////////////////////////////////////
// Timing Helper Functions
////////////////////////////////////////

pub fn time_nanos_one<F>(mut func: F) -> f64
where
    F: FnMut(),
{
    let start = Instant::now();
    func();
    let elapsed = start.elapsed();

    let secs = elapsed.as_secs() as f64;
    let nanos = elapsed.subsec_nanos() as f64;

    secs * 1e9 + nanos
}

pub fn time_nanos<F>(reps: u32, mut func: F) -> f64
where
    F: FnMut(),
{
    let nanos = time_nanos_one(|| {
        for _ in 0..reps {
            func();
        }
    });

    nanos / (reps as f64)
}

pub fn time_nanos_slice<T, F>(reps: u32, input: &[T], mut func: F) -> f64
where
    F: FnMut(&T),
{
    let nanos = time_nanos(reps, || {
        for value in input {
            func(value);
        }
    });

    nanos / (input.len() as f64)
}

////////////////////////////////////////
// Experiment 1
////////////////////////////////////////

pub struct Spec1<'a, T: 'a> {
    mode: OutputMode,
    family: (&'a str, u32, bool),
    input: (u32, &'a [T]),
}

impl<'a, T: 'a> Spec1<'a, T> {
    pub fn sample<F>(&self, mut func: F)
    where
        F: FnMut(&T) -> u32,
    {
        let (scheme, bits, is_128) = self.family;
        let (reps, input) = self.input;

        let input = test::black_box(input);

        let mut state = 0;
        let nanos = time_nanos_slice(reps, input, |value| state ^= func(value));
        let _ = test::black_box(state);

        match self.mode {
            OutputMode::Pretty => {
                println!(
                    "Scheme: {}, input bit-length: {}, 128-bit: {}; ns/value: {:.6}",
                    scheme, bits, is_128, nanos
                );
            }
            OutputMode::Csv => {
                let is_128 = if is_128 { "TRUE" } else { "FALSE" };
                println!("{},{},{},{}", scheme, bits, is_128, nanos);
            }
        }
    }
}

pub fn prepare_input_chunked<'a, 'b, T, F>(
    raw: &'a [u8],
    size: usize,
    n: usize,
    vec: &'b mut Vec<Vec<T>>,
    mut func: F,
) -> Vec<&'b [T]>
where
    T: Clone + Default,
    F: FnMut(&[u8], &mut [T]),
{
    for chunk in raw.chunks(size * n) {
        if chunk.len() < size * n {
            continue;
        }

        let mut value = vec![Default::default(); n];
        func(chunk, &mut value);
        vec.push(value);
    }

    vec.iter().map(|vec| &vec[..]).collect()
}

pub fn experiment_1(mode: OutputMode, input_raw: &[u8]) {
    let input_raw = &input_raw[..input_raw.len() & !15];

    let mut input_32 = vec![0; input_raw.len() / 4];
    let mut input_64 = vec![0; input_raw.len() / 8];

    BigEndian::read_u32_into(input_raw, &mut input_32);
    BigEndian::read_u64_into(input_raw, &mut input_64);

    let input_128 = input_64
        .chunks(2)
        .map(|x| (u128::from(x[0]) << 64) | u128::from(x[1]))
        .collect::<Vec<_>>();

    let input_30 = input_32
        .iter()
        .map(|&x| x & ((1 << 30) - 1))
        .collect::<Vec<_>>();
    let input_60 = input_64
        .iter()
        .map(|&x| x & ((1 << 60) - 1))
        .collect::<Vec<_>>();

    let reps = 100;

    if mode.is_csv() {
        println!("scheme,bits,is128,nanos");
    }

    // Multiply-Shift

    {
        let spec = Spec1 {
            mode,
            family: ("shift", 32, false),
            input: (reps, &input_32[..]),
        };

        let a = test::black_box(0x3bca40c7);

        spec.sample(|&x| imp::shift_u32(20, a, x));
    }
    {
        let spec = Spec1 {
            mode,
            family: ("shift", 64, false),
            input: (reps, &input_64[..]),
        };

        let a = test::black_box(0xa570f20b9bd5adfb);

        spec.sample(|&x| imp::shift_u64(20, a, x) as u32);
    }
    {
        let spec = Spec1 {
            mode,
            family: ("shift", 128, true),
            input: (reps, &input_128[..]),
        };

        let a = test::black_box(0x2cb56e50f9538749b4a1648382ba0d59);

        spec.sample(|&x| imp::shift_u128_128(20, a, x) as u32);
    }
    {
        let spec = Spec1 {
            mode,
            family: ("shift-strong", 32, false),
            input: (reps, &input_32[..]),
        };

        let a = test::black_box(0x9cb37f1a);
        let b = test::black_box(0x2d8b1736);

        spec.sample(|&x| imp::shift_strong_u32(20, a, b, x) as u32);
    }
    {
        let spec = Spec1 {
            mode,
            family: ("shift-strong", 64, true),
            input: (reps, &input_64[..]),
        };

        let a = test::black_box(0x6865db19e3d1b464);
        let b = test::black_box(0x583bc159d427a991);

        spec.sample(|&x| imp::shift_strong_u64_128(20, a, b, x) as u32);
    }

    // Multiply-Mod-Prime

    {
        let spec = Spec1 {
            mode,
            family: ("mmp", 30, false),
            input: (reps, &input_30[..]),
        };

        let a = test::black_box(0x40ed8147);
        let b = test::black_box(0x64b07a26);

        spec.sample(|&x| imp::mmp_p31_u30(20, a, b, x));
    }
    {
        let spec = Spec1 {
            mode,
            family: ("mmp-triple", 64, false),
            input: (reps, &input_64[..]),
        };

        let a = test::black_box([0x68dc5b2d, 0x29ad0bce, 0x278a331a]);
        let b = test::black_box([0x3e4f5b23, 0x2e47ea16, 0x3c665bad]);

        spec.sample(|&x| imp::mmp_p31_u64(20, a, b, x));
    }
    {
        let spec = Spec1 {
            mode,
            family: ("mmp", 60, true),
            input: (reps, &input_60[..]),
        };

        let a = test::black_box(0x02f52fcd0b6474c3);
        let b = test::black_box(0x0cb11e6766f6e421);

        spec.sample(|&x| imp::mmp_p61_u60_128(20, a, b, x) as u32);
    }
    {
        let spec = Spec1 {
            mode,
            family: ("mmp", 64, false),
            input: (reps, &input_64[..]),
        };

        let a = test::black_box([0xc543be39, 0xf663c8a4, 0x017193ad]);
        let b = test::black_box([0x180375ec, 0xd6fbb57d, 0x0010c0af]);

        spec.sample(|&x| imp::mmp_p89_u64(20, a, b, x) as u32);
    }

    // String Hashing

    for n in 1..=64 {
        let mut vec = Vec::new();
        let input = prepare_input_chunked(input_raw, 4, n, &mut vec, BigEndian::read_u32_into);

        let spec = Spec1 {
            mode,
            family: ("vector-shift", (32 * n) as u32, false),
            input: (reps, &input[..]),
        };

        let a = test::black_box([
            0x63b92c3f6df33488,
            0xa2207bc53adff964,
            0xaec2dca88ddb1e71,
            0x79e19e87fa120cd8,
            0x85bd7747f7d1493f,
            0xfdde9f44942c2df6,
            0x7ef56cb9766c7bc6,
            0xdec9e842facb5ba5,
            0x46f60b65eb67e0cc,
            0x6997be6404ca980b,
            0x8c0dbf8d1edc9e70,
            0xe49f1e9651cd3d49,
            0x58f51b8593abac4a,
            0x1c2aa379835987a3,
            0x92436454c98acd6f,
            0xf15831f1005edb4b,
            0xce7e34a170f36676,
            0x8e0684e862399c99,
            0x44aa63cc74e9d839,
            0x1ca6f7fdc88bd05b,
            0x1e1b5d120571c33f,
            0xa6e3aad03a3ff85f,
            0x8ae600281f7019d9,
            0x8559f1287c8bdaf7,
            0xdf63429a69a3b57b,
            0x4a4b9478b5a14152,
            0xaa94a0588831596f,
            0x21e11fe26c62825b,
            0x7a5392426a883bdb,
            0x2f3dcaa473d8222b,
            0xebe8bfe52c1bbca7,
            0xa5783fcb6379c163,
            0x4aa81d8a54b34ff4,
            0xefedb73f6cba9878,
            0x56182b429bc1d9f3,
            0xfa1ea680e285753f,
            0x21883ba4784126a2,
            0x52a07eee2741d3c9,
            0x322a1706fbcd82e7,
            0xec396c42fe6c180d,
            0x5c4819bf3abf0554,
            0x02331e406dc18024,
            0xd8c4e83242264407,
            0x51652714bd893042,
            0x7d5839310d07efe3,
            0xaf6f52fe3072a8f8,
            0x1c4fc232dd6ff784,
            0x9c9d5a588e373486,
            0x6962398ddf75a9e4,
            0x6dfe23682b96a306,
            0x02a9ffbcf9a2549f,
            0x171495363537cbb4,
            0x0e33ee376f9c6c7a,
            0x7e866b96944047b9,
            0x59ec7828c2c71193,
            0x93c8b1adc117d060,
            0x92b721f7bbf2b356,
            0xe14f685a4a6e66b7,
            0xda4eeef5ca762528,
            0xee00e49829361caa,
            0xba45113f9d873b85,
            0x0dfad687c9ebd0b8,
            0x97d359cd16140081,
            0x1d53acb7d0ce02aa,
            0xc6f31d83502a08d0,
        ]);

        spec.sample(|&chunk| {
            let mut h = imp::VectorShiftU32D64::new(a);
            for &x in chunk {
                h.write_u32(x);
            }
            h.finish(20)
        });
    }

    for n in 1..=32 {
        let mut vec = Vec::new();
        let input = prepare_input_chunked(input_raw, 8, n, &mut vec, BigEndian::read_u64_into);

        let spec = Spec1 {
            mode,
            family: ("pair-shift", (64 * n) as u32, false),
            input: (reps, &input[..]),
        };

        let a = test::black_box([
            0xa32b511bb9419925,
            0x468967dfa5b55d7c,
            0xd42a4cfdeaccd43c,
            0x3c2c20e3f28f94ad,
            0xce58ab0fc65d6b53,
            0xa24f83440516d39e,
            0xfeda02478b1da9bb,
            0x0a1c38a336c2f53f,
            0xee08871d414ea1e4,
            0x751f29778f19e95e,
            0x40714e646dcda33a,
            0xb304dbe1cd04d2ac,
            0x4c58ef616d8f044f,
            0xd006a9b5e0dc2623,
            0x1e9d6de78875186e,
            0x4c7c6c3f07eb6795,
            0x1503435a2323b6de,
            0x697bc32cadc36151,
            0xef2942f4cc29ce0d,
            0x09aadd479d1e4147,
            0x77a506902fc4e94c,
            0x35601d50f726e15c,
            0x359fbdab75f704ec,
            0x08b069380425ddcb,
            0x77071ac116b7bfe2,
            0x0f1fe1f375365ab0,
            0x1df5d02088d82064,
            0x373a6593a7b533dd,
            0xddca0594cabad3fa,
            0xafa30a4218f2473b,
            0xbac3eb0c71667dfd,
            0x73d944d2aeaa1269,
            0x9b3993f6d476ee21,
            0x1d0082cc5add5c6b,
            0xe1c721ec67b9f8d1,
            0x4dd3ae5399c02295,
            0x8c22156793c91933,
            0x539298bdc22fa4d3,
            0x518e460a4cebf181,
            0x28d7214e330ab8f4,
            0x09bc35ac293702d0,
            0x9f2e084081b677bc,
            0x31b981b366dd76e8,
            0xf8411798adebd9f9,
            0x4d7935d75ffa99be,
            0x4a5058b71a2170c3,
            0x16e68a8922ce1dfa,
            0x1b26ad0a35d2745d,
            0x12a7113f0927dca3,
            0x3ebc8b7f8b09920b,
            0xde2de731b4800c4e,
            0x0897fa405ec8cca4,
            0xf839242a1cda43c5,
            0xfb8b84894d9d4947,
            0x392d27343c4f233d,
            0x9d606bb797002b7f,
            0x8ee61bbd1967b081,
            0xfc60e7cd5f76f82c,
            0x91cbb2891a10527a,
            0xcfd9ead6bbe3b6c8,
            0xc091e4ed4cf36d45,
            0xf44ac339e3d30263,
            0x7dfecb5fda4973ad,
            0xc176a7a265736f18,
            0xa8ce3b04fbd2e1d3,
        ]);

        spec.sample(|&chunk| {
            let mut h = imp::PairShiftU64D32::new(a);
            for &x in chunk {
                h.write_u64(x);
            }
            h.finish(20) as u32
        });
    }

    for n in 1..=48 {
        let mut vec = Vec::new();
        let input = prepare_input_chunked(input_raw, 8, n, &mut vec, BigEndian::read_u64_into);

        let spec = Spec1 {
            mode,
            family: ("poly", (64 * n) as u32, false),
            input: (reps, &input[..]),
        };

        let a = test::black_box([0x0b9ba445, 0x417af2f0, 0x0057491f]);
        let b = test::black_box([0x7955e3d0, 0x9bc12da3, 0x00199f87]);
        let c = test::black_box([0xaf04903c, 0xa17b4a10, 0x01db4b2a]);

        spec.sample(|&chunk| {
            let mut h = imp::PolyU64::new(a, b, c);
            for &x in chunk {
                h.write_u64(x);
            }
            h.finish(20) as u32
        });
    }

    for n in 1..=48 {
        let mut vec = Vec::new();
        let input = prepare_input_chunked(input_raw, 8, n, &mut vec, BigEndian::read_u64_into);

        let spec = Spec1 {
            mode,
            family: ("poly-shift-triple", (64 * n) as u32, false),
            input: (reps, &input[..]),
        };

        let a = test::black_box([0x8ef01a402b94d1c0, 0xb51e6c8268b54d1e, 0x03de6480bede297c]);
        let b = test::black_box([0xd50d4b3992ece1d9, 0x03a9c4958c1da245, 0x8b24cd3ab504d6dd]);
        let c = test::black_box([0xb96bffb, 0xf3a4f6ed, 0x00304d5]);

        spec.sample(|&chunk| {
            let mut h = imp::PolyShiftU64::new(a, b, c);
            for &x in chunk {
                h.write_u64(x);
            }
            h.finish(20) as u32
        });
    }

    for n in 1..=48 {
        let mut vec = Vec::new();
        let input = prepare_input_chunked(input_raw, 8, n, &mut vec, BigEndian::read_u64_into);

        let spec = Spec1 {
            mode,
            family: ("preproc-poly", (64 * n) as u32, false),
            input: (reps, &input[..]),
        };

        let prep1 = test::black_box([
            0xd4f3ea9a2272d2c9,
            0xdc3760cf448d30de,
            0x829a1ad6e0abd369,
            0x230f22c05f31fafc,
            0x002d73bc6ac9c0fc,
            0x419c1546bb4977a7,
            0xfea2830da72e0e6d,
            0xa00338b990b45a8c,
            0xe0de174e92cf5e99,
            0x36418152d5968837,
            0x2a2a7405dac587dd,
            0x25574d5fd34af777,
            0x5f18f14d3412b17d,
            0x1676f95bacafc489,
            0xed3434ec09e4b62d,
            0x56e99b95cb0d4f9d,
            0x54c88c976f310672,
            0xfa2766750bbf1f25,
            0x65002b5b3acd700c,
            0xc05148d06acae7d5,
            0xc0610f45410d5256,
            0x25319fde69181971,
            0x8f7dc42720036b89,
            0x6cecd60153308cc5,
            0x02d75542ffd5c577,
            0xd46a35bbb01dde07,
            0x24c68493c6415a2b,
            0xa1dfd559ae8df2dd,
            0xe1d2be95d97103c6,
            0x6faae1b39a370207,
            0x7cb936383ea8f636,
            0x35a877ba36684a49,
            0x391d0d8071296d85,
            0x1940afc3115a74fe,
            0x9db6814ee4e7c24c,
            0xd1bbd8a2001640ac,
            0x11607bc954d1230e,
            0x55aeacb12b50f86d,
            0xc5dc3deb1d59a85a,
            0x8de127f87198bf1d,
            0xfaee0ebc89ad2800,
            0x83673b37d48744e4,
            0x92151b4f93a7af0d,
            0x842179950de5f38b,
            0x8d546a4f1d6acd5d,
            0x120310df82213b5c,
            0xd7ad98d66a4e3d0d,
            0xfd5bef050ff4a4d3,
            0x8621bb888e337211,
            0x75e82bfc266664a9,
            0x7e71425364b3073c,
            0x92d689dc0c2ed4eb,
            0x59e7c793769396b8,
            0x91f81934ca3ab4fe,
            0x8174e8f77fa652ad,
            0xba8400831008b6c1,
            0x3e3688a0abcaf0ae,
            0xd5a7b3900d9338bf,
            0xbd9ef501de44419d,
            0x41a458647e47d983,
            0x032a1474ff8ebfa7,
            0x508f3131ba0cbf6b,
            0x2cc6638bdabeed00,
            0xd64f3d02ad071cfd,
            0xf9da197c7c01a0c6,
        ]);
        let prep2 = test::black_box([
            0x294bb7f532dbd709,
            0x670c0d41304eb2f1,
            0x0b96b34742fb10eb,
            0xed95ba02eeeb659c,
            0x0012a024c15b0549,
            0xa8bfaa87980ce9be,
            0xf9bab0b2f2186d76,
            0x617623d3b002b90b,
            0x895068358779ce81,
            0xec122ed2ff2547fb,
            0x71a5296e12e1e89d,
            0xdba1deb3733180ec,
            0x6e672becd9eab262,
            0xdf843b38de2bced4,
            0x1aabf5b285966939,
            0x02e7579b88a6da96,
            0xa1e16c1d4abb89dd,
            0x4a0cebb617f42c32,
            0xb2ae462b89e36d43,
            0x7f30887ff26c18b7,
            0x51b709353a779bae,
            0xea74b3e11996364a,
            0x84abb5910b75b4ce,
            0xb03e324fbb354317,
            0xe03aade036376b5a,
            0xaf4c33010cb9a594,
            0x6dbf0d4a4896e540,
            0x037a4c06fad66b97,
            0x00a1135ec4555bac,
            0x01739917452a48d8,
            0x28b12cc96cb64d1c,
            0x98070ebf86ec5e79,
            0xc55ef31503ff8c4a,
            0xf9b52f06630847e3,
            0x3fada88ff1b136ca,
            0xbdc8e4b29bdb9c61,
            0x6eee6c3579a3d829,
            0xa892da6df62cc4bd,
            0xbdbd589fd800ba7e,
            0xa0927cda42f179f5,
            0xc597e0052d095ac3,
            0x9f5cc40389c138c1,
            0x7894e62a9edb1e67,
            0x45a2ad2fa22b70f6,
            0x8c77ecf89997376c,
            0xc63ca3f89f4a4fdc,
            0x471bb9e6c708c349,
            0xa7df821991d508d1,
            0x80e599de1192f2ca,
            0x130293d5f2114833,
            0xb45679c069ed61dc,
            0x38320009fdf56727,
            0x41d29ccc5b4a23d0,
            0xba2a07687599b6ac,
            0x3882402c4f88cdac,
            0x1aa06ddb9575ce70,
            0x143508533fcd2590,
            0xb2722b1a7b40a47b,
            0xa44e7d5966fd764b,
            0x56563507e31a9cc3,
            0xb3f42b919b571c96,
            0x21269ff066daca37,
            0xc6a79bdbe45a356a,
            0x7406c10cada8c543,
            0x87565863da322aac,
        ]);
        let a = test::black_box([0xec9f5086, 0xb59700c9, 0x00b9156b]);
        let b = test::black_box([0xf2dcaf8a, 0x2fbd1937, 0x019dfd97]);
        let c = test::black_box([0x71976d5e, 0xe5ee27a3, 0x00528ef6]);

        spec.sample(|&chunk| {
            let mut h = imp::PreprocPolyU64D32::new(prep1, prep2, a, b, c);
            for &x in chunk {
                h.write_u64(x);
            }
            h.finish(20) as u32
        });
    }

    #[allow(deprecated)]
    for n in 1..=384 {
        let mut vec = Vec::new();
        let input = prepare_input_chunked(input_raw, 1, n, &mut vec, |src, dst| {
            dst.copy_from_slice(src)
        });

        let spec = Spec1 {
            mode,
            family: ("siphash", (8 * n) as u32, false),
            input: (reps, &input[..]),
        };

        let a = test::black_box(0xdaec7826dea3bfbc);
        let b = test::black_box(0xa4cc9c56ee7d4797);

        spec.sample(|&chunk| {
            let mut h = SipHasher::new_with_keys(a, b);
            h.write(chunk);
            (h.finish() & ((1 << 20) - 1)) as u32
        });
    }
}

////////////////////////////////////////
// Experiment 2
////////////////////////////////////////

pub struct Spec2<'a, T: 'a> {
    config: (OutputMode, usize),
    family: &'a str,
    input: (u32, &'a [T]),
}

impl<'a, T: 'a> Spec2<'a, T> {
    pub fn sample<F>(&self, mut func: F)
    where
        F: FnMut(&[T]) -> u32,
    {
        let (mode, input_len) = self.config;
        let family = self.family;
        let (reps, input) = self.input;

        let input = test::black_box(input);

        let mut state = 0;
        let nanos = time_nanos(reps, || state ^= func(input));
        let _ = test::black_box(state);

        let nanos = nanos / (input_len as f64);

        match mode {
            OutputMode::Pretty => {
                println!("Family: {}; ns/value: {:.6}", family, nanos);
            }
            OutputMode::Csv => {
                println!("{},{}", family, nanos);
            }
        }
    }
}

pub fn experiment_2(mode: OutputMode, input_raw: &[u8]) {
    let input_8 = &input_raw[..input_raw.len() & !7];

    let mut input_64 = vec![0; input_8.len() / 8];

    BigEndian::read_u64_into(&input_8, &mut input_64[..]);

    let input_64 = test::black_box(&input_64[..]);

    let reps = 400;

    let config = (mode, input_8.len());

    if mode.is_csv() {
        println!("family,nanos");
    }

    {
        let spec = Spec2 {
            config,
            family: "poly",
            input: (reps, input_64),
        };

        let a = test::black_box([0x62b2da6d, 0x8826958f, 0x0ec048cd]);
        let b = test::black_box([0x9f7fe744, 0x94dddebf, 0x2b0d2821]);
        let c = test::black_box([0x02f6a761, 0xa607ade8, 0x27f45a1d]);

        spec.sample(|input| {
            let mut h = imp::PolyU64::new(a, b, c);
            for &x in input {
                h.write_u64(x);
            }
            h.finish(20) as u32
        });
    }
    {
        let spec = Spec2 {
            config,
            family: "poly-shift-triple",
            input: (reps, input_64),
        };

        let a = test::black_box([0x793d61ae7fdd32ab, 0x6012b7e4dc15941d, 0xc8572426ef9b5203]);
        let b = test::black_box([0x7b55dd4f434ac74a, 0x116034079f9a5ee8, 0xe55a53261f00ed6d]);
        let c = test::black_box([0xb1cae7e6, 0x70417c04, 0xf909b86d]);

        spec.sample(|input| {
            let mut h = imp::PolyShiftU64::new(a, b, c);
            for &x in input {
                h.write_u64(x);
            }
            h.finish(20)
        });
    }
    {
        let spec = Spec2 {
            config,
            family: "preproc-poly",
            input: (reps, input_64),
        };

        let prep1 = test::black_box([
            0xb2711dd1f64b11f8,
            0xa14d82ab5a17a6a7,
            0x2a93b430a4992ecd,
            0x478d54dd95322dc8,
            0x41b70e06aba2a434,
            0xfa16083099115ce7,
            0xbf9aa999ad86e918,
            0xc182f82b09eb0e2a,
            0x2d9f64c72f895ab3,
            0x889ac95554080d42,
            0x57fa5710a3482d8b,
            0x0356b40c6593d200,
            0xeb3cc7e866a821e8,
            0xa5b09ae9ed8abd19,
            0x628479fad8e5aefe,
            0x08b15b87aeb80865,
            0x5b94b902a6273d6f,
            0x58ed60cdad5651c8,
            0x0e883756ac34847d,
            0x739e08bece8c1afb,
            0xb3fd247af07604c0,
            0x53ac495f406c98e3,
            0x6f969543cd4f652b,
            0x8b5b7c4e1d87400e,
            0xb857f456c2f695e1,
            0x1d8fd52c332062a9,
            0x0d0d3bef996787a1,
            0x1fc05e7cde96c73c,
            0x18d16e10828de465,
            0x11d8545114360920,
            0x446e5d00562d27e8,
            0x28f59dac02e2a336,
            0xbf09edfd40bb79d0,
            0xbd2fe3bff64efecc,
            0x8c74ad84e2a70842,
            0xbe08794d0b8d89e0,
            0x3b0c98919f6c6104,
            0xacdac38359f8d4b9,
            0xcbef66432123c5bc,
            0xe6572cc09f691188,
            0xece5d497148c85e5,
            0x17c1e238cfec60bd,
            0xad3bc375f413c2ff,
            0x51648ef39d1819dd,
            0x8b983699da46f1fa,
            0x1c5bb59cdaf97c1c,
            0xb872814cf25012f6,
            0x9914c1319eb22f4a,
            0xf7821e2908c45974,
            0xc2cffd9e09c2320f,
            0x2f2316512c9a0adf,
            0x199dec7b341cc47b,
            0xde99127d37cb878b,
            0x8086184a5d5a64fd,
            0x98ea7944f742dcd4,
            0x68f8ae1a582bf3cd,
            0x959c39d090470cd8,
            0x6254aeb7abca0be3,
            0x7d88fa326d566050,
            0x7591d7a9c90630ec,
            0x982ff47e76a58933,
            0xb3bfd40af6c7435f,
            0x3a06bacea9b614bf,
            0x629ead52af26c2e3,
            0x4f0968c29a85dec0,
        ]);
        let prep2 = test::black_box([
            0x5ed5c3bacc741f65,
            0x9bab6f845c42113d,
            0x7bfbf889a2fd0a1f,
            0x7c8f812a26d3483e,
            0xfe28802f9294dabb,
            0x5c4fa9d04638c151,
            0x4a4016d82c8482ea,
            0x17c03363cbb6ed1f,
            0xeeb304ce9600813b,
            0xa62b338db7aa3e90,
            0x990369f27c286dec,
            0xd0b4f4c272c33667,
            0x81dd673631f66d42,
            0x1bb1e2e6b56a34f4,
            0x5309e2ea6ac0597d,
            0x9fd0b4a045e121b7,
            0x32027f16ac80bf5c,
            0x0e4ccb092bdd0a04,
            0x299b55861877be0f,
            0x444b0c5ba96910fe,
            0x4f0a5fa446fc4776,
            0xebff80ff93e24f06,
            0x256853b2694580f9,
            0xee6157146dd7b9ac,
            0x520762619e2bb113,
            0x727008b113c006f9,
            0xb9a478d4ac3f0a00,
            0xec68c7cbde302bee,
            0xc26eb87ad1d92332,
            0xdb02f1917e5c79cf,
            0x9f596f9f50448998,
            0xe0e5ead89800e238,
            0x9f354b1c6c142e6d,
            0xe80d8f81ab451d5b,
            0x88a8ac6c4b0b9283,
            0xf8aa151d5bdb34b9,
            0xc928e36d201086c0,
            0xc15e5f5ae43fd0e6,
            0x11275e76f1b221c3,
            0x224832c6e1189b20,
            0xe645b9e64c38ef65,
            0x201c6beea4ebcd1f,
            0x24bc1ea58cf557fa,
            0xf928755c28bc3009,
            0x831b51c8ebb6bfe5,
            0x6decd1200ae5db52,
            0xd8db99fbe290009d,
            0xa2d5902820846630,
            0x9161a602e034bf54,
            0xacb96a363d9f2136,
            0x1d2786aaebc7bfcb,
            0xc19cfc53d36baf9a,
            0x1f65c728faf63fbb,
            0x8c7eb5ee6a102764,
            0xe4325d41d3c35352,
            0xb4ae714575249b48,
            0xc2ba608b89207799,
            0x8e1623b9268ae83a,
            0xae63c36da58b4287,
            0x25cb2de57c95576e,
            0xaa13007bb2930ac8,
            0x37316d818c945055,
            0xf5df774f5db918d4,
            0x05a73d52342e4c47,
            0xcc8a6557eda0d91f,
        ]);
        let a = test::black_box([0x640a3992, 0xa9aec943, 0x061d4e0c]);
        let b = test::black_box([0x4ff935a2, 0x0e0c9fcb, 0x1d3fb360]);
        let c = test::black_box([0xb6aaea65, 0x7f9eefc7, 0x320577f3]);

        spec.sample(|input| {
            let mut h = imp::PreprocPolyU64D32::new(prep1, prep2, a, b, c);
            for &x in input {
                h.write_u64(x);
            }
            h.finish(20) as u32
        });
    }

    // SipHash

    #[allow(deprecated)]
    {
        let spec = Spec2 {
            config,
            family: "siphash",
            input: (reps, input_8),
        };

        let a = test::black_box(0x3b67cbb8b09e78f0);
        let b = test::black_box(0xd7f3a93cead49a81);

        spec.sample(|input| {
            let mut h = SipHasher::new_with_keys(a, b);
            h.write(input);
            (h.finish() & ((1 << 20) - 1)) as u32
        });
    }
}
