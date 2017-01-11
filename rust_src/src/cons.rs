extern crate libc;

use std::os::raw::c_char;
use std::ptr;
use std::mem;

use lisp::{LispObject, LispType, XTYPE, XUNTAG, Qt, Qnil, LispSubr, EmacsInt, PvecType,
           VectorLikeHeader, PSEUDOVECTOR_AREA_BITS, CHECK_TYPE};

extern "C" {
    static Qconsp: LispObject;
    fn CHECK_IMPURE(obj: LispObject, ptr: *const libc::c_void);
}


fn CONSP(x: LispObject) -> bool {
    XTYPE(x) == LispType::Lisp_Cons
}

fn Fconsp(object: LispObject) -> LispObject {
    if CONSP(object) { unsafe { Qt } } else { Qnil }
}

lazy_static! {
    pub static ref Sconsp: LispSubr = LispSubr {
        header: VectorLikeHeader {
            size: ((PvecType::PVEC_SUBR as libc::c_int) <<
                   PSEUDOVECTOR_AREA_BITS) as libc::ptrdiff_t,
        },
        function: (Fconsp as *const libc::c_void),
        min_args: 1,
        max_args: 1,
        symbol_name: ("consp\0".as_ptr()) as *const c_char,
        intspec: ptr::null(),
        doc: ("Return t if OBJECT is a cons cell.

(fn OBJECT)\0".as_ptr()) as *const c_char,
    };
}

/// Represents a cons cell, or GC bookkeeping for cons cells.
///
/// A cons cell is pair of two pointers, used to build linked lists in
/// lisp.
///
/// # C Porting Notes
///
/// The equivalent C struct is `Lisp_Cons`. Note that the second field
/// may be used as the cdr or GC bookkeeping.
// TODO: this should be aligned to 8 bytes.
#[repr(C)]
#[allow(unused_variables)]
struct LispCons {
    /// Car of this cons cell.
    car: LispObject,
    /// Cdr of this cons cell, or the chain used for the free list.
    cdr: LispObject,
}

// alloc.c uses a union for `Lisp_Cons`, which we emulate with an
// opaque struct.
#[repr(C)]
#[allow(dead_code)]
pub struct LispConsChain {
    chain: *mut LispConsChain,
}

/// Extract the LispCons data from an elisp value.
fn XCONS(a: LispObject) -> *mut LispCons {
    debug_assert!(CONSP(a));
    unsafe { mem::transmute(XUNTAG(a, LispType::Lisp_Cons as libc::c_int)) }
}

/// Set the car of a cons cell.
fn XSETCAR(c: LispObject, n: LispObject) {
    let cons_cell = XCONS(c);
    unsafe {
        (*cons_cell).car = n;
    }
}

/// Set the cdr of a cons cell.
fn XSETCDR(c: LispObject, n: LispObject) {
    let cons_cell = XCONS(c);
    unsafe {
        (*cons_cell).cdr = n;
    }
}

#[no_mangle]
pub extern "C" fn Fsetcar(cell: LispObject, newcar: LispObject) -> LispObject {
    unsafe {
        CHECK_TYPE(CONSP(cell), Qconsp, cell);
        CHECK_IMPURE(cell, XCONS(cell) as *const libc::c_void);
    }

    XSETCAR(cell, newcar);
    newcar
}

lazy_static! {
    pub static ref Ssetcar: LispSubr = LispSubr {
        header: VectorLikeHeader {
            size: ((PvecType::PVEC_SUBR as libc::c_int) <<
                   PSEUDOVECTOR_AREA_BITS) as libc::ptrdiff_t,
        },
        function: (Fsetcar as *const libc::c_void),
        min_args: 2,
        max_args: 2,
        symbol_name: ("setcar\0".as_ptr()) as *const c_char,
        intspec: ptr::null(),
        doc: ("Set the car of CELL to be NEWCAR. Returns NEWCAR.

(fn CELL NEWCAR)\0".as_ptr()) as *const c_char,
    };
}

#[no_mangle]
pub extern "C" fn Fsetcdr(cell: LispObject, newcar: LispObject) -> LispObject {
    unsafe {
        CHECK_TYPE(CONSP(cell), Qconsp, cell);
        CHECK_IMPURE(cell, XCONS(cell) as *const libc::c_void);
    }

    XSETCDR(cell, newcar);
    newcar
}

lazy_static! {
    pub static ref Ssetcdr: LispSubr = LispSubr {
        header: VectorLikeHeader {
            size: ((PvecType::PVEC_SUBR as libc::c_int) <<
                   PSEUDOVECTOR_AREA_BITS) as libc::ptrdiff_t,
        },
        function: (Fsetcdr as *const libc::c_void),
        min_args: 2,
        max_args: 2,
        symbol_name: ("setcdr\0".as_ptr()) as *const c_char,
        intspec: ptr::null(),
        doc: ("Set the cdr of CELL to be NEWCDR.  Returns NEWCDR.

(fn CELL NEWCDR)\0".as_ptr()) as *const c_char,
    };
}

// When scanning the C stack for live Lisp objects, Emacs keeps track of
// what memory allocated via lisp_malloc and lisp_align_malloc is intended
// for what purpose.  This enumeration specifies the type of memory.
//
// # Porting Notes
//
// `mem_type` in C.
#[repr(C)]
enum MemType {
    MEM_TYPE_NON_LISP,
    MEM_TYPE_BUFFER,
    MEM_TYPE_CONS,
    MEM_TYPE_STRING,
    MEM_TYPE_MISC,
    MEM_TYPE_SYMBOL,
    MEM_TYPE_FLOAT,
    // Since all non-bool pseudovectors are small enough to be
    // allocated from vector blocks, this memory type denotes
    // large regular vectors and large bool pseudovectors.
    MEM_TYPE_VECTORLIKE,
    // Special type to denote vector blocks.
    MEM_TYPE_VECTOR_BLOCK,
    // Special type to denote reserved memory.
    MEM_TYPE_SPARE,
}

extern "C" {
    /// Construct a LispObject from a value or address.
    ///
    /// # Porting Notes
    ///
    /// This function also replaces the C macros `XSETCONS`,
    /// `XSETVECTOR`, `XSETSTRING`, `XSETFLOAT` and `XSETMISC`.
    fn make_lisp_ptr(ptr: *mut libc::c_void, ty: LispType) -> LispObject;
    fn lisp_align_malloc(nbytes: libc::size_t, ty: MemType) -> *mut libc::c_void;
    /// Free-list of Lisp_Cons structures.
    static mut cons_free_list: *mut LispConsChain;
    static mut consing_since_gc: EmacsInt;
    static mut total_free_conses: EmacsInt;
}

const CONS_BLOCK_SIZE: usize = 100;

/// An unsigned integer type representing a fixed-length bit sequence,
/// suitable for bool vector words, GC mark bits, etc.
type bits_word = libc::size_t;

const BITS_PER_BITS_WORD: usize = 8 * 8;

/// The ConsBlock is used to store cons cells.
///
/// We allocate new ConsBlock values when needed. Cons cells reclaimed
/// by GC are put on a free list to be reallocated before allocating
/// any new cons cells from the latest ConsBlock.
///
/// # Porting Notes
///
/// This is `cons_block` in C.
#[repr(C)]
struct ConsBlock {
    conses: [LispCons; CONS_BLOCK_SIZE],
    gcmarkbits: [bits_word; 1 + CONS_BLOCK_SIZE / BITS_PER_BITS_WORD],
    next: *mut ConsBlock,
}

fn Fcons(car: LispObject, cdr: LispObject) -> LispObject {
    // MALLOC_BLOCK_INPUT; is a no-op.

    let mut val: LispObject;

    val = 1;
    unsafe {
        if !cons_free_list.is_null() {
            // Use the current head of the free list for this cons
            // cell, and remove it from the free list.
            val = make_lisp_ptr(cons_free_list as *mut libc::c_void, LispType::Lisp_Cons);
            cons_free_list = (*cons_free_list).chain;
        } else {
            // Otherwise, we need to malloc some meory.


        }
    }

    XSETCAR(val, car);
    XSETCDR(val, cdr);

    // assert marked

    unsafe {
        consing_since_gc += mem::size_of::<LispCons>() as i64;
        total_free_conses += 1;
        // cons_cells_consed++
    }

    val
}

lazy_static! {
    pub static ref Scons: LispSubr = LispSubr {
        header: VectorLikeHeader {
            size: ((PvecType::PVEC_SUBR as libc::c_int) <<
                   PSEUDOVECTOR_AREA_BITS) as libc::ptrdiff_t,
        },
        function: (Fcons as *const libc::c_void),
        min_args: 2,
        max_args: 2,
        symbol_name: ("rust-cons\0".as_ptr()) as *const c_char,
        intspec: ptr::null(),
        doc: ("Create a new cons, give it CAR and CDR as components, and return it.

(fn CAR CDR)\0".as_ptr()) as *const c_char,
    };
}
