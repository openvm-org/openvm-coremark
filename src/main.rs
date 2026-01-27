// CoreMark benchmark ported to Rust for OpenVM
// Original: Copyright 2018 Embedded Microprocessor Benchmark Consortium (EEMBC)
// Licensed under Apache License 2.0
// Rust port for OpenVM zkVM - exact match to C original

#![allow(unused_parens)]

use core::cell::RefCell;
use openvm::io::reveal_u32;

// Configuration
const TOTAL_DATA_SIZE: usize = 2000;
const ITERATIONS: u32 = 1000;

// Type aliases matching CoreMark types
type EeU8 = u8;
type EeS16 = i16;
type EeU16 = u16;
type EeS32 = i32;
type EeU32 = u32;

// Algorithm IDs
const ID_LIST: u32 = 1 << 0;
const ID_MATRIX: u32 = 1 << 1;
const ID_STATE: u32 = 1 << 2;

// Matrix types
type MatDat = i16;
type MatRes = i32;

// ============================================================================
// CRC functions - exact match to C original
// ============================================================================

fn crcu8(data: EeU8, crc: EeU16) -> EeU16 {
    let mut data = data;
    let mut crc = crc;
    let mut carry: u8;

    for _ in 0..8 {
        let x16: EeU8 = ((data & 1) ^ ((crc as EeU8) & 1));
        data >>= 1;

        if x16 == 1 {
            crc ^= 0x4002;
            carry = 1;
        } else {
            carry = 0;
        }
        crc >>= 1;
        if carry != 0 {
            crc |= 0x8000;
        } else {
            crc &= 0x7fff;
        }
    }
    crc
}

fn crcu16(newval: EeU16, crc: EeU16) -> EeU16 {
    let crc = crcu8((newval) as EeU8, crc);
    crcu8(((newval) >> 8) as EeU8, crc)
}

fn crc16(newval: EeS16, crc: EeU16) -> EeU16 {
    crcu16(newval as EeU16, crc)
}

fn crcu32(newval: EeU32, crc: EeU16) -> EeU16 {
    let crc = crc16(newval as EeS16, crc);
    crc16((newval >> 16) as EeS16, crc)
}

// ============================================================================
// List data structures
// ============================================================================

#[derive(Clone, Copy, Default)]
struct ListData {
    data16: EeS16,
    idx: EeS16,
}

const LIST_NULL: usize = usize::MAX;

#[derive(Clone, Copy)]
struct ListNode {
    next: usize,
    info_idx: usize,
}

impl Default for ListNode {
    fn default() -> Self {
        ListNode {
            next: LIST_NULL,
            info_idx: 0,
        }
    }
}

// ============================================================================
// Matrix data structures
// ============================================================================

struct MatParams {
    n: usize,
    a: Vec<MatDat>,
    b: Vec<MatDat>,
    c: Vec<MatRes>,
}

// ============================================================================
// Core results and context - using RefCell for interior mutability
// ============================================================================

struct CoreContext {
    // Seeds
    seed1: EeS16,
    seed2: EeS16,
    seed3: EeS16,
    // Size per algorithm
    size: EeU32,
    // Iterations
    iterations: EeU32,
    // Algorithm mask
    execs: EeU32,
    // CRC values
    crc: EeU16,
    crclist: EeU16,
    crcmatrix: EeU16,
    crcstate: EeU16,
    // Error count
    err: EeS16,
    // List storage
    list_nodes: Vec<ListNode>,
    list_data: Vec<ListData>,
    list_head: usize,
    // Matrix params
    mat: MatParams,
    // State data
    state_memblock: Vec<EeU8>,
}

// ============================================================================
// Matrix functions - exact match to C original
// ============================================================================

#[inline]
fn matrix_clip(x: EeS32, y: bool) -> MatDat {
    if y {
        ((x) & 0x0ff) as MatDat
    } else {
        ((x) & 0x0ffff) as MatDat
    }
}

#[inline]
fn matrix_big(x: MatDat) -> MatDat {
    (0xf000i32 | (x as i32)) as MatDat
}

#[inline]
fn bit_extract(x: MatRes, from: u32, to: u32) -> MatRes {
    ((x) >> (from)) & (!(0xffffffffu32 << (to)) as MatRes)
}

fn matrix_add_const(n: usize, a: &mut [MatDat], val: MatDat) {
    for i in 0..n {
        for j in 0..n {
            a[i * n + j] = a[i * n + j].wrapping_add(val);
        }
    }
}

fn matrix_mul_const(n: usize, c: &mut [MatRes], a: &[MatDat], val: MatDat) {
    for i in 0..n {
        for j in 0..n {
            c[i * n + j] = (a[i * n + j] as MatRes) * (val as MatRes);
        }
    }
}

fn matrix_mul_vect(n: usize, c: &mut [MatRes], a: &[MatDat], b: &[MatDat]) {
    for i in 0..n {
        c[i] = 0;
        for j in 0..n {
            c[i] += (a[i * n + j] as MatRes) * (b[j] as MatRes);
        }
    }
}

fn matrix_mul_matrix(n: usize, c: &mut [MatRes], a: &[MatDat], b: &[MatDat]) {
    for i in 0..n {
        for j in 0..n {
            c[i * n + j] = 0;
            for k in 0..n {
                c[i * n + j] += (a[i * n + k] as MatRes) * (b[k * n + j] as MatRes);
            }
        }
    }
}

fn matrix_mul_matrix_bitextract(n: usize, c: &mut [MatRes], a: &[MatDat], b: &[MatDat]) {
    for i in 0..n {
        for j in 0..n {
            c[i * n + j] = 0;
            for k in 0..n {
                let tmp: MatRes = (a[i * n + k] as MatRes) * (b[k * n + j] as MatRes);
                c[i * n + j] += bit_extract(tmp, 2, 4) * bit_extract(tmp, 5, 7);
            }
        }
    }
}

fn matrix_sum(n: usize, c: &[MatRes], clipval: MatDat) -> EeS16 {
    let mut tmp: MatRes = 0;
    let mut prev: MatRes = 0;
    let mut ret: EeS16 = 0;

    for i in 0..n {
        for j in 0..n {
            let cur = c[i * n + j];
            tmp += cur;
            if tmp > (clipval as MatRes) {
                ret += 10;
                tmp = 0;
            } else {
                ret += if cur > prev { 1 } else { 0 };
            }
            prev = cur;
        }
    }
    ret
}

fn matrix_test(n: usize, c: &mut [MatRes], a: &mut [MatDat], b: &[MatDat], val: MatDat) -> EeS16 {
    let mut crc: EeU16 = 0;
    let clipval: MatDat = matrix_big(val);

    matrix_add_const(n, a, val);
    matrix_mul_const(n, c, a, val);
    crc = crc16(matrix_sum(n, c, clipval), crc);
    matrix_mul_vect(n, c, a, b);
    crc = crc16(matrix_sum(n, c, clipval), crc);
    matrix_mul_matrix(n, c, a, b);
    crc = crc16(matrix_sum(n, c, clipval), crc);
    matrix_mul_matrix_bitextract(n, c, a, b);
    crc = crc16(matrix_sum(n, c, clipval), crc);
    matrix_add_const(n, a, (-(val as i32)) as MatDat);

    crc as EeS16
}

fn core_bench_matrix(mat: &mut MatParams, seed: EeS16, crc: EeU16) -> EeU16 {
    let n = mat.n;
    let val: MatDat = seed as MatDat;
    let result = matrix_test(n, &mut mat.c, &mut mat.a, &mat.b, val);
    crc16(result, crc)
}

fn core_init_matrix(blksize: usize, seed: EeS32, mat: &mut MatParams) -> usize {
    let mut n: usize = 0;
    let mut seed = if seed == 0 { 1 } else { seed };
    let mut order: EeS32 = 1;

    // Calculate N
    let mut j: usize = 0;
    while j < blksize {
        n += 1;
        j = n * n * 2 * 4;
    }
    n -= 1;

    mat.a = vec![0; n * n];
    mat.b = vec![0; n * n];
    mat.c = vec![0; n * n];
    mat.n = n;

    for i in 0..n {
        for j in 0..n {
            seed = ((order.wrapping_mul(seed)) % 65536);
            let mut val: EeS32 = (seed + order);
            val = matrix_clip(val, false) as EeS32;
            mat.b[i * n + j] = val as MatDat;
            val = (val + order);
            val = matrix_clip(val, true) as EeS32;
            mat.a[i * n + j] = val as MatDat;
            order += 1;
        }
    }

    n
}

// ============================================================================
// State machine - exact match to C original
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
enum CoreState {
    Start = 0,
    Invalid = 1,
    S1 = 2,
    S2 = 3,
    Int = 4,
    Float = 5,
    Exponent = 6,
    Scientific = 7,
}

const NUM_CORE_STATES: usize = 8;

#[inline]
fn ee_isdigit(c: EeU8) -> bool {
    ((c >= b'0') & (c <= b'9'))
}

fn core_state_transition(
    memblock: &[EeU8],
    pos: &mut usize,
    transition_count: &mut [EeU32; NUM_CORE_STATES],
) -> CoreState {
    let mut state = CoreState::Start;

    while *pos < memblock.len() {
        let next_symbol = memblock[*pos];
        if next_symbol == 0 {
            break;
        }
        if state == CoreState::Invalid {
            break;
        }

        if next_symbol == b',' {
            *pos += 1;
            break;
        }

        match state {
            CoreState::Start => {
                if ee_isdigit(next_symbol) {
                    state = CoreState::Int;
                } else if next_symbol == b'+' || next_symbol == b'-' {
                    state = CoreState::S1;
                } else if next_symbol == b'.' {
                    state = CoreState::Float;
                } else {
                    state = CoreState::Invalid;
                    transition_count[CoreState::Invalid as usize] += 1;
                }
                transition_count[CoreState::Start as usize] += 1;
            }
            CoreState::S1 => {
                if ee_isdigit(next_symbol) {
                    state = CoreState::Int;
                    transition_count[CoreState::S1 as usize] += 1;
                } else if next_symbol == b'.' {
                    state = CoreState::Float;
                    transition_count[CoreState::S1 as usize] += 1;
                } else {
                    state = CoreState::Invalid;
                    transition_count[CoreState::S1 as usize] += 1;
                }
            }
            CoreState::Int => {
                if next_symbol == b'.' {
                    state = CoreState::Float;
                    transition_count[CoreState::Int as usize] += 1;
                } else if !ee_isdigit(next_symbol) {
                    state = CoreState::Invalid;
                    transition_count[CoreState::Int as usize] += 1;
                }
            }
            CoreState::Float => {
                if next_symbol == b'E' || next_symbol == b'e' {
                    state = CoreState::S2;
                    transition_count[CoreState::Float as usize] += 1;
                } else if !ee_isdigit(next_symbol) {
                    state = CoreState::Invalid;
                    transition_count[CoreState::Float as usize] += 1;
                }
            }
            CoreState::S2 => {
                if next_symbol == b'+' || next_symbol == b'-' {
                    state = CoreState::Exponent;
                    transition_count[CoreState::S2 as usize] += 1;
                } else {
                    state = CoreState::Invalid;
                    transition_count[CoreState::S2 as usize] += 1;
                }
            }
            CoreState::Exponent => {
                if ee_isdigit(next_symbol) {
                    state = CoreState::Scientific;
                    transition_count[CoreState::Exponent as usize] += 1;
                } else {
                    state = CoreState::Invalid;
                    transition_count[CoreState::Exponent as usize] += 1;
                }
            }
            CoreState::Scientific => {
                if !ee_isdigit(next_symbol) {
                    state = CoreState::Invalid;
                    transition_count[CoreState::Invalid as usize] += 1;
                }
            }
            CoreState::Invalid => break,
        }

        *pos += 1;
    }

    state
}

fn core_init_state(size: usize, mut seed: EeS16, p: &mut Vec<EeU8>) {
    static INTPAT: [&[u8]; 4] = [b"5012", b"1234", b"-874", b"+122"];
    static FLOATPAT: [&[u8]; 4] = [b"35.54400", b".1234500", b"-110.700", b"+0.64400"];
    static SCIPAT: [&[u8]; 4] = [b"5.500e+3", b"-.123e-2", b"-87e+832", b"+0.6e-12"];
    static ERRPAT: [&[u8]; 4] = [b"T0.3e-1F", b"-T.T++Tq", b"1T3.4e4z", b"34.0e-T^"];

    p.clear();
    p.resize(size, 0);

    let size = size - 1;
    let mut total: usize = 0;
    let mut next: usize = 0;
    let mut buf: &[u8] = &[];

    while (total + next + 1) < size {
        if next > 0 {
            for i in 0..next {
                p[total + i] = buf[i];
            }
            p[total + next] = b',';
            total += next + 1;
        }
        seed = seed.wrapping_add(1);
        match seed & 0x7 {
            0 | 1 | 2 => {
                buf = INTPAT[((seed >> 3) & 0x3) as usize];
                next = 4;
            }
            3 | 4 => {
                buf = FLOATPAT[((seed >> 3) & 0x3) as usize];
                next = 8;
            }
            5 | 6 => {
                buf = SCIPAT[((seed >> 3) & 0x3) as usize];
                next = 8;
            }
            7 => {
                buf = ERRPAT[((seed >> 3) & 0x3) as usize];
                next = 8;
            }
            _ => {}
        }
    }
}

fn core_bench_state(
    blksize: usize,
    memblock: &mut [EeU8],
    seed1: EeS16,
    seed2: EeS16,
    step: EeS16,
    mut crc: EeU16,
) -> EeU16 {
    let mut final_counts = [0u32; NUM_CORE_STATES];
    let mut track_counts = [0u32; NUM_CORE_STATES];

    // Run state machine over input
    let mut pos: usize = 0;
    while pos < blksize && memblock[pos] != 0 {
        let fstate = core_state_transition(memblock, &mut pos, &mut track_counts);
        final_counts[fstate as usize] += 1;
    }

    // Insert corruption
    let step_usize = step as usize;
    let mut p: usize = 0;
    while p < blksize {
        if memblock[p] != b',' {
            memblock[p] ^= seed1 as u8;
        }
        p += step_usize;
    }

    // Run state machine again
    pos = 0;
    while pos < blksize && memblock[pos] != 0 {
        let fstate = core_state_transition(memblock, &mut pos, &mut track_counts);
        final_counts[fstate as usize] += 1;
    }

    // Undo corruption
    p = 0;
    while p < blksize {
        if memblock[p] != b',' {
            memblock[p] ^= seed2 as u8;
        }
        p += step_usize;
    }

    // Calculate CRC
    for i in 0..NUM_CORE_STATES {
        crc = crcu32(final_counts[i], crc);
        crc = crcu32(track_counts[i], crc);
    }

    crc
}

// ============================================================================
// List functions - exact match to C original
// ============================================================================

fn copy_info(to: &mut ListData, from: &ListData) {
    to.data16 = from.data16;
    to.idx = from.idx;
}

fn core_list_find(
    nodes: &[ListNode],
    data: &[ListData],
    mut list: usize,
    info: &ListData,
) -> usize {
    if info.idx >= 0 {
        while list != LIST_NULL && data[nodes[list].info_idx].idx != info.idx {
            list = nodes[list].next;
        }
        return list;
    } else {
        while list != LIST_NULL && ((data[nodes[list].info_idx].data16 & 0xff) != info.data16) {
            list = nodes[list].next;
        }
        return list;
    }
}

fn core_list_reverse(nodes: &mut [ListNode], mut list: usize) -> usize {
    let mut next: usize = LIST_NULL;
    while list != LIST_NULL {
        let tmp = nodes[list].next;
        nodes[list].next = next;
        next = list;
        list = tmp;
    }
    next
}

fn core_list_remove(nodes: &mut [ListNode], item: usize) -> usize {
    let ret = nodes[item].next;
    // Swap info indices
    let tmp = nodes[item].info_idx;
    nodes[item].info_idx = nodes[ret].info_idx;
    nodes[ret].info_idx = tmp;
    // Eliminate item
    nodes[item].next = nodes[nodes[item].next].next;
    nodes[ret].next = LIST_NULL;
    ret
}

fn core_list_undo_remove(
    nodes: &mut [ListNode],
    item_removed: usize,
    item_modified: usize,
) -> usize {
    // Swap info indices
    let tmp = nodes[item_removed].info_idx;
    nodes[item_removed].info_idx = nodes[item_modified].info_idx;
    nodes[item_modified].info_idx = tmp;
    // Insert item back
    nodes[item_removed].next = nodes[item_modified].next;
    nodes[item_modified].next = item_removed;
    item_removed
}

fn core_list_insert_new(
    nodes: &mut [ListNode],
    data: &mut [ListData],
    insert_point: usize,
    info: &ListData,
    memblock_idx: &mut usize,
    datablock_idx: &mut usize,
    memblock_end: usize,
    datablock_end: usize,
) -> Option<usize> {
    // Match C exactly: if ((*memblock + 1) >= memblock_end)
    if (*memblock_idx + 1) >= memblock_end {
        return None;
    }
    if (*datablock_idx + 1) >= datablock_end {
        return None;
    }

    let newitem = *memblock_idx;
    *memblock_idx += 1;
    nodes[newitem].next = nodes[insert_point].next;
    nodes[insert_point].next = newitem;

    nodes[newitem].info_idx = *datablock_idx;
    copy_info(&mut data[*datablock_idx], info);
    *datablock_idx += 1;

    Some(newitem)
}

// calc_func - exact match to C original
// Modifies pdata in place to cache result
fn calc_func(ctx: &RefCell<CoreContext>, info_idx: usize) -> EeS16 {
    let data = ctx.borrow().list_data[info_idx].data16;
    let optype: EeU8 = ((data >> 7) & 1) as EeU8;

    if optype != 0 {
        // Cached, use cache
        return data & 0x007f;
    }

    // Calculate and cache
    let flag: EeS16 = data & 0x7;
    let mut dtype: EeS16 = ((data >> 3) & 0xf);
    dtype |= dtype << 4;

    let retval: EeS16;
    match flag {
        0 => {
            let dtype_adjusted = if dtype < 0x22 { 0x22 } else { dtype };
            // Extract immutable values before mutable borrow
            let (blksize, seed1, seed2, crc) = {
                let c = ctx.borrow();
                (c.size as usize, c.seed1, c.seed2, c.crc)
            };
            let result = {
                let mut c = ctx.borrow_mut();
                core_bench_state(
                    blksize,
                    &mut c.state_memblock,
                    seed1,
                    seed2,
                    dtype_adjusted,
                    crc,
                )
            };
            {
                let mut c = ctx.borrow_mut();
                if c.crcstate == 0 {
                    c.crcstate = result;
                }
            }
            retval = result as EeS16;
        }
        1 => {
            // Extract immutable values before mutable borrow
            let crc = ctx.borrow().crc;
            let result = {
                let mut c = ctx.borrow_mut();
                core_bench_matrix(&mut c.mat, dtype, crc)
            };
            {
                let mut c = ctx.borrow_mut();
                if c.crcmatrix == 0 {
                    c.crcmatrix = result;
                }
            }
            retval = result as EeS16;
        }
        _ => {
            retval = data;
        }
    }

    // Update CRC and cache result
    {
        let mut c = ctx.borrow_mut();
        c.crc = crcu16(retval as EeU16, c.crc);
        let masked_retval = retval & 0x007f;
        c.list_data[info_idx].data16 = (data & 0xff00u16 as i16) | 0x0080 | masked_retval;
    }

    retval & 0x007f
}

// cmp_complex - exact match to C original
fn cmp_complex(ctx: &RefCell<CoreContext>, p_info_idx: usize, q_info_idx: usize) -> EeS32 {
    let val1 = calc_func(ctx, p_info_idx);
    let val2 = calc_func(ctx, q_info_idx);
    (val1 - val2) as EeS32
}

// cmp_idx - exact match to C original
// When regen is true, regenerate data from backup (like passing NULL for res in C)
fn cmp_idx(ctx: &RefCell<CoreContext>, p_info_idx: usize, q_info_idx: usize, regen: bool) -> EeS32 {
    if regen {
        let mut c = ctx.borrow_mut();
        let a_data = c.list_data[p_info_idx].data16;
        c.list_data[p_info_idx].data16 = (a_data & 0xff00u16 as i16) | (0x00ff & (a_data >> 8));
        let b_data = c.list_data[q_info_idx].data16;
        c.list_data[q_info_idx].data16 = (b_data & 0xff00u16 as i16) | (0x00ff & (b_data >> 8));
    }
    let c = ctx.borrow();
    (c.list_data[p_info_idx].idx - c.list_data[q_info_idx].idx) as EeS32
}

// Merge sort - exact match to C original
enum CmpMode {
    ByIdx { regen: bool },
    Complex,
}

fn core_list_mergesort(ctx: &RefCell<CoreContext>, mut list: usize, mode: CmpMode) -> usize {
    let mut insize: i32 = 1;

    loop {
        let mut p = list;
        list = LIST_NULL;
        let mut tail = LIST_NULL;
        let mut nmerges: i32 = 0;

        while p != LIST_NULL {
            nmerges += 1;
            let mut q = p;
            let mut psize: i32 = 0;

            for _ in 0..insize {
                psize += 1;
                q = ctx.borrow().list_nodes[q].next;
                if q == LIST_NULL {
                    break;
                }
            }

            let mut qsize: i32 = insize;

            while psize > 0 || (qsize > 0 && q != LIST_NULL) {
                let e: usize;

                if psize == 0 {
                    e = q;
                    q = ctx.borrow().list_nodes[q].next;
                    qsize -= 1;
                } else if qsize == 0 || q == LIST_NULL {
                    e = p;
                    p = ctx.borrow().list_nodes[p].next;
                    psize -= 1;
                } else {
                    let p_info_idx = ctx.borrow().list_nodes[p].info_idx;
                    let q_info_idx = ctx.borrow().list_nodes[q].info_idx;

                    let cmp_result = match &mode {
                        CmpMode::ByIdx { regen } => cmp_idx(ctx, p_info_idx, q_info_idx, *regen),
                        CmpMode::Complex => cmp_complex(ctx, p_info_idx, q_info_idx),
                    };

                    if cmp_result <= 0 {
                        e = p;
                        p = ctx.borrow().list_nodes[p].next;
                        psize -= 1;
                    } else {
                        e = q;
                        q = ctx.borrow().list_nodes[q].next;
                        qsize -= 1;
                    }
                }

                if tail != LIST_NULL {
                    ctx.borrow_mut().list_nodes[tail].next = e;
                } else {
                    list = e;
                }
                tail = e;
            }

            p = q;
        }

        if tail != LIST_NULL {
            ctx.borrow_mut().list_nodes[tail].next = LIST_NULL;
        }

        if nmerges <= 1 {
            return list;
        }

        insize *= 2;
    }
}

fn core_list_init(blksize: usize, seed: EeS16, ctx: &RefCell<CoreContext>) -> usize {
    let per_item: usize = 16 + core::mem::size_of::<ListData>();
    let size: usize = (blksize / per_item) - 2;

    // Take ownership of vectors to work with them directly
    // Allocate size + 2 for physical storage (head + tail + size items potentially)
    // But set end pointers to match C: memblock_end = memblock + size
    let mut list_nodes = vec![ListNode::default(); size + 2];
    let mut list_data = vec![ListData::default(); size + 2];

    // Match C exactly: memblock_end = memblock + size (not size + 2)
    let memblock_end = size;
    let datablock_end = size;
    let mut memblock_idx: usize = 0;
    let mut datablock_idx: usize = 0;

    // Create list head
    let list: usize = memblock_idx;
    list_nodes[list].next = LIST_NULL;
    list_nodes[list].info_idx = datablock_idx;
    list_data[datablock_idx].idx = 0x0000;
    list_data[datablock_idx].data16 = 0x8080u16 as i16;
    memblock_idx += 1;
    datablock_idx += 1;

    // Create tail sentinel
    let mut info = ListData {
        idx: 0x7fff,
        data16: -1i16,
    };
    core_list_insert_new(
        &mut list_nodes,
        &mut list_data,
        list,
        &info,
        &mut memblock_idx,
        &mut datablock_idx,
        memblock_end,
        datablock_end,
    );

    // Insert size items
    // Note: info.idx stays 0x7fff from sentinel setup, matching C behavior
    for i in 0..size {
        let datpat: EeU16 = ((seed ^ (i as EeS16)) & 0xf) as EeU16;
        let dat: EeU16 = (datpat << 3) | ((i as EeU16) & 0x7);
        info.data16 = ((dat << 8) | dat) as EeS16;
        // info.idx NOT set here - C code doesn't set it either

        core_list_insert_new(
            &mut list_nodes,
            &mut list_data,
            list,
            &info,
            &mut memblock_idx,
            &mut datablock_idx,
            memblock_end,
            datablock_end,
        );
    }

    // Index the list - exact match to C i++ behavior
    {
        let mut finder = list_nodes[list].next;
        let mut i: usize = 1;
        while finder != LIST_NULL && list_nodes[finder].next != LIST_NULL {
            let info_idx = list_nodes[finder].info_idx;
            if i < size / 5 {
                // C: finder->info->idx = i++;
                list_data[info_idx].idx = i as EeS16;
                i += 1;
            } else {
                // C: pat = (ee_u16)(i++ ^ seed); then uses incremented i
                let pat: EeU16 = ((i as EeS16) ^ seed) as EeU16;
                i += 1;
                // Now i is incremented, matching C behavior
                list_data[info_idx].idx =
                    (0x3fff & ((((i & 0x07) << 8) as EeU16) | pat)) as EeS16;
            }
            finder = list_nodes[finder].next;
        }
    }

    // Put vectors back into context
    {
        let mut c = ctx.borrow_mut();
        c.list_nodes = list_nodes;
        c.list_data = list_data;
    }

    // Sort by index - C code passes NULL for res, which triggers regen
    // (though regen is a no-op for initial values since upper/lower bytes are equal)
    let sorted_list = core_list_mergesort(ctx, list, CmpMode::ByIdx { regen: true });
    ctx.borrow_mut().list_head = sorted_list;

    sorted_list
}

fn core_bench_list(ctx: &RefCell<CoreContext>, finder_idx: EeS16) -> EeU16 {
    let mut retval: EeU16 = 0;
    let mut found: EeU16 = 0;
    let mut missed: EeU16 = 0;

    let mut list = ctx.borrow().list_head;
    let find_num = ctx.borrow().seed3;
    let mut info = ListData {
        idx: finder_idx,
        data16: 0,
    };

    // Find <find_num> values in the list, and change the list each time
    for i in 0..find_num {
        info.data16 = (i & 0xff) as EeS16;

        let this_find = {
            let c = ctx.borrow();
            core_list_find(&c.list_nodes, &c.list_data, list, &info)
        };

        list = {
            let mut c = ctx.borrow_mut();
            core_list_reverse(&mut c.list_nodes, list)
        };

        if this_find == LIST_NULL {
            missed += 1;
            let c = ctx.borrow();
            let next = c.list_nodes[list].next;
            if next != LIST_NULL {
                retval += ((c.list_data[c.list_nodes[next].info_idx].data16 >> 8) & 1) as EeU16;
            }
        } else {
            found += 1;
            {
                let c = ctx.borrow();
                if (c.list_data[c.list_nodes[this_find].info_idx].data16 & 0x1) != 0 {
                    retval +=
                        ((c.list_data[c.list_nodes[this_find].info_idx].data16 >> 9) & 1) as EeU16;
                }
            }
            // Cache next item at head of list
            let next = ctx.borrow().list_nodes[this_find].next;
            if next != LIST_NULL {
                let mut c = ctx.borrow_mut();
                let finder = next;
                c.list_nodes[this_find].next = c.list_nodes[finder].next;
                c.list_nodes[finder].next = c.list_nodes[list].next;
                c.list_nodes[list].next = finder;
            }
        }

        if info.idx >= 0 {
            info.idx += 1;
        }
    }

    retval += found * 4 - missed;

    // Sort by data content (complex comparison) if finder_idx > 0
    if finder_idx > 0 {
        list = core_list_mergesort(ctx, list, CmpMode::Complex);
    }

    // Remove one item
    let remover = {
        let mut c = ctx.borrow_mut();
        let next = c.list_nodes[list].next;
        core_list_remove(&mut c.list_nodes, next)
    };

    // CRC data content of list from location of index N forward
    // NOTE: Original C code uses list->info->data16 in the loop, not finder->info
    let finder_start = {
        let c = ctx.borrow();
        let found_idx = core_list_find(&c.list_nodes, &c.list_data, list, &info);
        if found_idx == LIST_NULL {
            c.list_nodes[list].next
        } else {
            found_idx
        }
    };

    {
        let c = ctx.borrow();
        let mut finder = finder_start;
        while finder != LIST_NULL {
            // Original: retval = crc16(list->info->data16, retval);
            retval = crc16(c.list_data[c.list_nodes[list].info_idx].data16, retval);
            finder = c.list_nodes[finder].next;
        }
    }

    // Undo remove
    {
        let mut c = ctx.borrow_mut();
        let next = c.list_nodes[list].next;
        core_list_undo_remove(&mut c.list_nodes, remover, next);
    }

    // Sort by index (with regen - NULL res in C)
    list = core_list_mergesort(ctx, list, CmpMode::ByIdx { regen: true });

    // CRC data content of list
    // NOTE: Original C code uses list->info->data16 in the loop, not finder->info
    {
        let c = ctx.borrow();
        let mut finder = c.list_nodes[list].next;
        while finder != LIST_NULL {
            // Original: retval = crc16(list->info->data16, retval);
            retval = crc16(c.list_data[c.list_nodes[list].info_idx].data16, retval);
            finder = c.list_nodes[finder].next;
        }
    }

    ctx.borrow_mut().list_head = list;
    retval
}

// ============================================================================
// Main iterate function
// ============================================================================

fn iterate(ctx: &RefCell<CoreContext>) {
    {
        let mut c = ctx.borrow_mut();
        c.crc = 0;
        c.crclist = 0;
        c.crcmatrix = 0;
        c.crcstate = 0;
    }

    let iterations = ctx.borrow().iterations;

    for i in 0..iterations {
        let crc = core_bench_list(ctx, 1);
        {
            let mut c = ctx.borrow_mut();
            c.crc = crcu16(crc, c.crc);
        }

        let crc = core_bench_list(ctx, -1);
        {
            let mut c = ctx.borrow_mut();
            c.crc = crcu16(crc, c.crc);
            if i == 0 {
                c.crclist = c.crc;
            }
        }
    }
}

// ============================================================================
// Known CRC values for validation
// ============================================================================

static LIST_KNOWN_CRC: [EeU16; 5] = [0xd4b0, 0x3340, 0x6a79, 0xe714, 0xe3c1];
static MATRIX_KNOWN_CRC: [EeU16; 5] = [0xbe52, 0x1199, 0x5608, 0x1fd7, 0x0747];
static STATE_KNOWN_CRC: [EeU16; 5] = [0x5e47, 0x39bf, 0xe5a4, 0x8e3a, 0x8d84];

// ============================================================================
// Main entry point
// ============================================================================

fn main() {
    let iterations: EeU32 = ITERATIONS;
    let num_algorithms: usize = 3;
    let size: usize = TOTAL_DATA_SIZE / num_algorithms;

    // Create context with RefCell for interior mutability
    // Seeds for PERFORMANCE_RUN (2K data size) - must match C volatile values
    // seed1=0, seed2=0, seed3=0x66, size=666 gives seedcrc=0xe9f5
    let ctx = RefCell::new(CoreContext {
        seed1: 0,
        seed2: 0,
        seed3: 0x66,
        size: size as EeU32,
        iterations,
        execs: ID_LIST | ID_MATRIX | ID_STATE,
        crc: 0,
        crclist: 0,
        crcmatrix: 0,
        crcstate: 0,
        err: 0,
        list_nodes: Vec::new(),
        list_data: Vec::new(),
        list_head: LIST_NULL,
        mat: MatParams {
            n: 0,
            a: Vec::new(),
            b: Vec::new(),
            c: Vec::new(),
        },
        state_memblock: Vec::new(),
    });

    // Initialize algorithms
    let execs = ctx.borrow().execs;
    let seed1 = ctx.borrow().seed1;
    let seed2 = ctx.borrow().seed2;

    if (execs & ID_LIST) != 0 {
        core_list_init(size, seed1, &ctx);
    }

    if (execs & ID_MATRIX) != 0 {
        let seed = (seed1 as EeS32) | ((seed2 as EeS32) << 16);
        let mut c = ctx.borrow_mut();
        core_init_matrix(size, seed, &mut c.mat);
    }

    if (execs & ID_STATE) != 0 {
        let mut c = ctx.borrow_mut();
        core_init_state(size, seed1, &mut c.state_memblock);
    }

    // Run benchmark
    iterate(&ctx);

    // Calculate seed CRC for validation
    let mut seedcrc: EeU16 = 0;
    {
        let c = ctx.borrow();
        seedcrc = crc16(c.seed1, seedcrc);
        seedcrc = crc16(c.seed2, seedcrc);
        seedcrc = crc16(c.seed3, seedcrc);
        seedcrc = crc16(size as EeS16, seedcrc);
    }

    // Determine known configuration
    let known_id: i32 = match seedcrc {
        0x8a02 => 0, // 6k performance
        0x7b05 => 1, // 6k validation
        0x4eaf => 2, // profile generation
        0xe9f5 => 3, // 2K performance
        0x18f2 => 4, // 2K validation
        _ => -1,
    };

    // Validate results
    let mut total_errors: i32 = 0;
    {
        let c = ctx.borrow();
        if known_id >= 0 {
            let kid = known_id as usize;
            if (c.execs & ID_LIST) != 0 && c.crclist != LIST_KNOWN_CRC[kid] {
                total_errors += 1;
            }
            if (c.execs & ID_MATRIX) != 0 && c.crcmatrix != MATRIX_KNOWN_CRC[kid] {
                total_errors += 1;
            }
            if (c.execs & ID_STATE) != 0 && c.crcstate != STATE_KNOWN_CRC[kid] {
                total_errors += 1;
            }
        } else {
            total_errors = -1;
        }
    }

    // Output results via reveal_u32 (zkVM output mechanism)
    // Slot 0: iterations
    // Slot 1: final CRC | (error count << 16)
    // Slot 2: list CRC
    // Slot 3: matrix CRC
    // Slot 4: state CRC
    // Slot 5: seed CRC (for validation)
    // Slot 6: known_id (config identifier)
    // Slot 7: reserved
    let c = ctx.borrow();
    reveal_u32(c.iterations, 0);
    let output1 = (c.crc as u32) | (((total_errors as u32) & 0xffff) << 16);
    reveal_u32(output1, 1);
    reveal_u32(c.crclist as u32, 2);
    reveal_u32(c.crcmatrix as u32, 3);
    reveal_u32(c.crcstate as u32, 4);
    reveal_u32(seedcrc as u32, 5);
    reveal_u32(known_id as u32, 6);
    reveal_u32(total_errors as u32, 7);
}
