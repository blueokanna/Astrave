use jni::{JavaVM, objects::JObject, sys};

// Dart keeps the application context's global reference alive for this call.
#[unsafe(no_mangle)]
unsafe extern "C" fn astrave_init_android_tls(vm: *mut sys::JavaVM, context: sys::jobject) -> bool {
    if vm.is_null() || context.is_null() {
        return false;
    }
    let vm = unsafe { JavaVM::from_raw(vm) };
    vm.attach_current_thread(|env| {
        let raw = env.get_raw();
        let local = unsafe { ((**raw).v1_2.NewLocalRef)(raw, context) };
        if local.is_null() {
            return Ok(false);
        }
        let context = unsafe { JObject::from_raw(env, local) };
        rustls_platform_verifier::android::init_with_env(env, context)?;
        Ok::<_, jni::errors::Error>(true)
    })
    .unwrap_or(false)
}
