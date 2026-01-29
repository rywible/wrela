use crate::actor::{drop_actor, drop_pending, drop_pool};
use crate::class::drop_class;
use crate::iter::drop_iter;
use crate::list::drop_list;
use crate::map::drop_map;
use crate::string::drop_string;
use crate::result::drop_result;
use crate::value::{drop_boxed_int, TypeId};
use std::sync::atomic::AtomicU32;

#[repr(C)]
pub struct ObjHeader {
    pub rc: AtomicU32,
    pub type_id: u32,
}

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
        x if x == TypeId::BoxedInt as u32 => unsafe { drop_boxed_int(ptr) },
        _ => {
            if type_id >= TypeId::UserBase as u32 {
                unsafe { drop_class(ptr) };
            }
        }
    }
}
