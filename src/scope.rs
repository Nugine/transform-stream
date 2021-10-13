use std::cell::Cell;
use std::ptr;

#[derive(Clone, Copy)]
struct Scope {
    place: *mut (),
    id: u64,
}

impl Scope {
    const INVALID: Self = Self {
        place: ptr::null_mut(),
        id: 0,
    };
}

thread_local! {
    static SCOPE: Cell<Scope> = Cell::new(Scope::INVALID);
}

pub fn enter_scope<T, R>(id: u64, place: &mut Option<T>, f: impl FnOnce() -> R) -> R {
    struct Guard(Scope);

    impl Drop for Guard {
        fn drop(&mut self) {
            SCOPE.with(|p| p.set(self.0))
        }
    }

    let place = place as *mut _ as *mut ();
    let scope = Scope { id, place };
    let _guard = SCOPE.with(|p| Guard(p.replace(scope)));
    f()
}

pub unsafe fn in_scope<'a, T>(id: u64) -> &'a mut Option<T> {
    let scope = SCOPE.with(|p| p.get());
    assert!(scope.id == id, "invalid usage");
    &mut *scope.place.cast()
}
