// Copyright 2016 lazy-static.rs Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

/*!
A macro for declaring lazily evaluated statics.

Using this macro, it is possible to have `static`s that require code to be
executed at runtime in order to be initialized.
This includes anything requiring heap allocations, like vectors or hash maps,
as well as anything that requires function calls to be computed.

# Syntax

```ignore
lazy_static! {
    [pub] static ref NAME_1: TYPE_1 = EXPR_1;
    [pub] static ref NAME_2: TYPE_2 = EXPR_2;
    ...
    [pub] static ref NAME_N: TYPE_N = EXPR_N;
}
```

Attributes (including doc comments) are supported as well:

```rust
# #[macro_use]
# extern crate lazy_static;
# fn main() {
lazy_static! {
    /// This is an example for using doc comment attributes
    static ref EXAMPLE: u8 = 42;
}
# }
```

# Semantics

For a given `static ref NAME: TYPE = EXPR;`, the macro generates a unique type that
implements `Deref<TYPE>` and stores it in a static with name `NAME`. (Attributes end up
attaching to this type.)

On first deref, `EXPR` gets evaluated and stored internally, such that all further derefs
can return a reference to the same object. Note that this can lead to deadlocks
if you have multiple lazy statics that depend on each other in their initialization.

Apart from the lazy initialization, the resulting "static ref" variables
have generally the same properties as regular "static" variables:

- Any type in them needs to fulfill the `Sync` trait.
- If the type has a destructor, then it will not run when the process exits.

# Example

Using the macro:

```rust
#[macro_use]
extern crate lazy_static;

use std::collections::HashMap;

lazy_static! {
    static ref HASHMAP: HashMap<u32, &'static str> = {
        let mut m = HashMap::new();
        m.insert(0, "foo");
        m.insert(1, "bar");
        m.insert(2, "baz");
        m
    };
    static ref COUNT: usize = HASHMAP.len();
    static ref NUMBER: u32 = times_two(21);
}

fn times_two(n: u32) -> u32 { n * 2 }

fn main() {
    println!("The map has {} entries.", *COUNT);
    println!("The entry for `0` is \"{}\".", HASHMAP.get(&0).unwrap());
    println!("A expensive calculation on a static results in: {}.", *NUMBER);
}
```

# Implementation details

The `Deref` implementation uses a hidden static variable that is guarded by a atomic check on each access. On stable Rust, the macro may need to allocate each static on the heap.

*/

#![cfg_attr(feature="nightly", feature(const_fn, allow_internal_unstable, core_intrinsics))]

#![doc(html_root_url = "https://docs.rs/lazy_static/0.2.6")]
#![no_std]

#[cfg(not(feature="nightly"))]
#[doc(hidden)]
pub mod lazy;

#[cfg(all(feature="nightly", not(feature="spin_no_std")))]
#[path="nightly_lazy.rs"]
#[doc(hidden)]
pub mod lazy;

#[cfg(all(feature="nightly", feature="spin_no_std"))]
#[path="core_lazy.rs"]
#[doc(hidden)]
pub mod lazy;

#[doc(hidden)]
pub use core::ops::Deref as __Deref;

#[macro_export]
#[cfg_attr(feature="nightly", allow_internal_unstable)]
#[doc(hidden)]
macro_rules! __lazy_static_internal {
    ($(#[$attr:meta])* static ref $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        __lazy_static_internal!(@PRIV, $(#[$attr])* static ref $N : $T = $e; $($t)*);
    };
    ($(#[$attr:meta])* pub static ref $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        __lazy_static_internal!(@PUB, $(#[$attr])* static ref $N : $T = $e; $($t)*);
    };
    (@$VIS:ident, $(#[$attr:meta])* static ref $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        __lazy_static_internal!(@MAKE TY, $VIS, $(#[$attr])*, $N);
        impl $crate::__Deref for $N {
            type Target = $T;
            #[allow(unsafe_code)]
            fn deref(&self) -> &$T {
                unsafe {
                    #[inline(always)]
                    fn __static_ref_initialize() -> $T { $e }

                    #[inline(always)]
                    unsafe fn __stability() -> &'static $T {
                        __lazy_static_create!(LAZY, $T);
                        LAZY.get(__static_ref_initialize)
                    }
                    __stability()
                }
            }
        }
        impl $crate::LazyStatic for $N {
            fn initialize(lazy: &Self) {
                let _ = &**lazy;
            }
        }
        __lazy_static_internal!($($t)*);
    };
    (@MAKE TY, PUB, $(#[$attr:meta])*, $N:ident) => {
        #[allow(missing_copy_implementations)]
        #[allow(non_camel_case_types)]
        #[allow(dead_code)]
        $(#[$attr])*
        pub struct $N {__private_field: ()}
        #[doc(hidden)]
        pub static $N: $N = $N {__private_field: ()};
    };
    (@MAKE TY, PRIV, $(#[$attr:meta])*, $N:ident) => {
        #[allow(missing_copy_implementations)]
        #[allow(non_camel_case_types)]
        #[allow(dead_code)]
        $(#[$attr])*
        struct $N {__private_field: ()}
        #[doc(hidden)]
        static $N: $N = $N {__private_field: ()};
    };
    () => ()
}

#[macro_export]
#[cfg_attr(feature="nightly", allow_internal_unstable)]
macro_rules! lazy_static {
    ($(#[$attr:meta])* static ref $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        __lazy_static_internal!(@PRIV, $(#[$attr])* static ref $N : $T = $e; $($t)*);
    };
    ($(#[$attr:meta])* pub static ref $N:ident : $T:ty = $e:expr; $($t:tt)*) => {
        __lazy_static_internal!(@PUB, $(#[$attr])* static ref $N : $T = $e; $($t)*);
    };
    () => ()
}

/// Support trait for enabling a few common operation on lazy static values.
///
/// This is implemented by each defined lazy static, and
/// used by the free functions in this crate.
pub trait LazyStatic {
    #[doc(hidden)]
    fn initialize(lazy: &Self);
}

/// Takes a shared reference to a lazy static and initializes
/// it if it has not been already.
///
/// This can be used to control the initialization point of a lazy static.
///
/// Example:
///
/// ```rust
/// #[macro_use]
/// extern crate lazy_static;
///
/// lazy_static! {
///     static ref BUFFER: Vec<u8> = (0..65537).collect();
/// }
///
/// fn main() {
///     lazy_static::initialize(&BUFFER);
///
///     // ...
///     work_with_initialized_data(&BUFFER);
/// }
/// # fn work_with_initialized_data(_: &[u8]) {}
/// ```
pub fn initialize<T: LazyStatic>(lazy: &T) {
    LazyStatic::initialize(lazy);
}
