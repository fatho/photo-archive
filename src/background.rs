use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Sender, Receiver, channel};

struct CallbackInfo<R, C> {
    result_var: Arc<Mutex<Option<R>>>,
    callback: Option<C>,
}

trait Callbackable {
    fn run(&mut self);
}

impl<R, C> Callbackable for CallbackInfo<R, C> where C: FnOnce(R) -> () {
    fn run(&mut self) {
        let result = self.result_var.lock().expect("Should be unlocked").take().unwrap();
        (self.callback.take().unwrap())(result);
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct CallbackKey(usize);

thread_local! {
    static NEXT_KEY: RefCell<CallbackKey> = RefCell::new(CallbackKey(0));

    static RXTX: (Sender<CallbackKey>, Receiver<CallbackKey>) = channel();

    static CALLBACKS: RefCell<HashMap<CallbackKey, Box<dyn Callbackable>>> = RefCell::new(HashMap::new());
}

fn receive() -> glib::Continue {
    RXTX.with(|(_sender, receiver)| {
        // If this code is executed, we know that at least one callback has already been enqueued.
        if let Ok(callback_key) = receiver.try_recv() {
            CALLBACKS.with(|callbacks_cell| {
                let mut callback_map = callbacks_cell.borrow_mut();
                let mut callback = callback_map.remove(&callback_key).unwrap();
                callback.run();
            });
        }
    });
    glib::Continue(false)
}

pub struct TaskHandle<R> {
    callback_key: CallbackKey,
    callback_sender: Sender<CallbackKey>,
    result_var: Arc<Mutex<Option<R>>>,
    _result: std::marker::PhantomData<R>
}

impl<R: Send> TaskHandle<R> {
    pub fn provide(self, value: R) {
        let mut result = self.result_var.lock().unwrap();
        *result = Some(value);

        self.callback_sender.send(self.callback_key).unwrap();
        glib::idle_add(receive);
    }
}

pub fn register_background_task<C, R>(callback: C) -> TaskHandle<R> where C: FnOnce(R) -> () + 'static, R: 'static {
    let result_var = Arc::new(Mutex::new(None));
    let key = NEXT_KEY.with(|key_var| {
        let mut key_mut = key_var.borrow_mut();
        let key = *key_mut;
        *key_mut = CallbackKey(key.0 + 1);
        key
    });
    let sender = RXTX.with(|(sender, _receiver)| sender.clone());
    let callback_info = CallbackInfo {
        result_var: result_var.clone(),
        callback: Some(callback)
    };
    CALLBACKS.with(|callbacks| {
        callbacks.borrow_mut().insert(key, Box::new(callback_info))
    });
    TaskHandle {
        callback_key: key,
        callback_sender: sender,
        result_var: result_var,
        _result: std::marker::PhantomData,
    }
}
