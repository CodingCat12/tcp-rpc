use async_trait::async_trait;
use bincode::{Decode, Encode};

use std::fmt::Debug;

#[async_trait]
pub trait Request: Encode + Decode<()> + Debug {
    type Resp: Response;

    async fn handle(self) -> Self::Resp;
}

pub trait Response: Encode + Decode<()> + Debug {}

// Response impl's for basic types
macro_rules! impl_resp {
    ( $name:ident < $($gen:ident),* > $( where $($w:tt)* )? ) => {
        impl<$($gen),*> Response for $name<$($gen),*> $( where $($w)* )? {}
    };

    ( $name:ident $( where $($w:tt)* )? ) => {
        impl Response for $name $( where $($w)* )? {}
    };

    ( $head:tt $($tail:tt)* ) => {
        impl_resp! { $head }
        impl_resp! { $($tail)* }
    };

    () => {};
}

// integer types
impl_resp! { usize u8 u16 u32 u64 u128 }
impl_resp! { isize i8 i16 i32 i64 i128 }

// floating-point types
impl_resp! { f32 f64 }

// miscellaneous
impl_resp! { String bool char }

impl_resp!(Vec<T> where T: Debug + Encode + Decode<()>);
impl_resp!(Option<T> where T: Debug + Encode + Decode<()>);
impl_resp!(Result<T, E> where T: Debug + Encode + Decode<()>, E: Debug + Encode + Decode<()>);

impl Response for () {}
