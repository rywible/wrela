use crate::actor::{drop_actor, drop_pending, drop_pool};
use crate::bytes::drop_bytes;
use crate::class::drop_class;
use crate::iter::drop_iter;
use crate::list::drop_list;
use crate::map::drop_map;
use crate::math::{drop_mat3, drop_mat4, drop_vec};
use crate::result::drop_result;
use crate::string::drop_string;
use crate::value::{TypeId, drop_boxed_int};
use std::sync::atomic::AtomicU32;

#[repr(C)]
pub struct ObjHeader {
    pub rc: AtomicU32,
    pub type_id: u32,
}

/// # Safety
/// `ptr` must either be null or point to a live heap object allocated by this runtime
/// with a valid `ObjHeader` type tag. The pointed object must not be dropped elsewhere.
pub unsafe fn drop_object(ptr: *mut ObjHeader) {
    if ptr.is_null() {
        return;
    }
    let type_id = unsafe { (*ptr).type_id };
    match type_id {
        x if x == TypeId::String as u32 => drop_string(ptr),
        x if x == TypeId::List as u32 => drop_list(ptr),
        x if x == TypeId::Map as u32 => drop_map(ptr),
        x if x == TypeId::Actor as u32 => unsafe { drop_actor(ptr) },
        x if x == TypeId::Pending as u32 => unsafe { drop_pending(ptr) },
        x if x == TypeId::Iterator as u32 => unsafe { drop_iter(ptr) },
        x if x == TypeId::Result as u32 => unsafe { drop_result(ptr) },
        x if x == TypeId::Pool as u32 => unsafe { drop_pool(ptr) },
        x if x == TypeId::Bytes as u32 => drop_bytes(ptr),
        x if x == TypeId::BoxedInteger as u32 => unsafe { drop_boxed_int(ptr) },
        x if x == TypeId::Vec2 as u32
            || x == TypeId::Vec3 as u32
            || x == TypeId::Vec4 as u32
            || x == TypeId::Quat as u32 =>
        {
            unsafe { drop_vec(ptr) }
        }
        x if x == TypeId::Mat3 as u32 => unsafe { drop_mat3(ptr) },
        x if x == TypeId::Mat4 as u32 => unsafe { drop_mat4(ptr) },
        _ => {
            if type_id >= TypeId::UserBase as u32 {
                unsafe { drop_class(ptr) };
            }
        }
    }
}
