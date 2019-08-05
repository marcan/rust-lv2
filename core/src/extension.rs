//! Means to extend the interface of a plugin.
use crate::plugin::Plugin;
use crate::UriBound;
use std::any::Any;

/// A trait for marking a type as an LV2 Plugin extension.
///
/// # Unsafety
///
/// Failing to meet any of these requirements will lead to undefined behavior when the host will
/// try to load the extension for any given plugin.
///
pub unsafe trait Extension<P: Plugin>: UriBound {
    /// The raw data structure defined by the extension, as returned by the plugin's `extension_data()` method.
    ///
    /// It can be set to any static value, it is up to the implementer of the
    /// extension to set it correctly:
    ///
    /// * The struct being pointed to must be `#[repr(C)]` and have exactly the same fields as the one defined by the LV2 Extension specification.
    /// * The struct being pointed to must be correctly initialized, as defined by the LV2 Extension specification;
    /// * The URI associated to this extension must be exactly the same as the one defined in the LV2 Extension specification, and must also be the one tied to the struct being pointed to by `RAW_DATA`.
    const RAW_DATA: &'static (dyn Any + 'static);
}

/// Generate a method body for a plugin's `extension_data` method.
///
/// In most cases, you don't need to implement [`extension_data`](plugin/trait.Plugin.html#method.extension_data) yourself, since most dynamic extension objects implement [`Extension`](extension/trait.Extension.html). An example:
///
///     use lv2_core::plugin::{Plugin, PluginInfo, FeatureContainer};
///     use lv2_core::extension::Extension;
///     use lv2_core::{UriBound, match_extensions};
///
///     use std::any::Any;
///     use std::ffi::{CString, CStr};
///
///     // ######################
///     // Defining the extension
///     // ######################
///
///     trait MyExtension {
///         fn foo(&self) -> f32;
///     }
///         
///     #[doc(hidden)]
///     unsafe extern "C" fn extern_foo<P: MyExtension>(handle: *const P) -> f32 {
///         handle.as_ref().unwrap().foo()
///     }
///
///     #[repr(C)]
///     struct MyExtensionInterface<P: MyExtension> {
///         pub foo: unsafe extern "C" fn(input: *const P) -> f32,
///     }
///
///     unsafe impl UriBound for dyn MyExtension {
///         const URI: &'static [u8] = b"urn:my-extension\0";
///     }
///
///     unsafe impl<P: Plugin + MyExtension> Extension<P> for dyn MyExtension {
///         const RAW_DATA: &'static dyn Any = &MyExtensionInterface {
///             foo: extern_foo::<P>,
///         };
///     }
///
///     // ###################
///     // Defining the plugin
///     // ###################
///
///     struct MyPlugin {
///         data: f32,    
///     }
///
///     unsafe impl UriBound for MyPlugin {
///         const URI: &'static [u8] = b"urn:my-plugin\0";
///     }
///
///     impl Plugin for MyPlugin {
///         type Ports = ();
///
///         fn new(_: &PluginInfo, _: FeatureContainer) -> Self {
///             MyPlugin {
///                 data: 42.0
///             }
///         }
///
///         fn run(&mut self, _: &mut ()) {}
///
///         fn extension_data(uri: &CStr) -> Option<&'static dyn Any> {
///             match_extensions![uri, dyn MyExtension]
///         }
///     }
///
///     impl MyExtension for MyPlugin {
///         fn foo(&self) -> f32 {
///             self.data
///         }
///     }
///
///     // #########################################
///     // Simulated host code that tests everything
///     // #########################################
///
///     let my_plugin = MyPlugin {
///         data: 17.0,  
///     };
///     let interface: &MyExtensionInterface<MyPlugin> = MyPlugin
///         ::extension_data(<dyn MyExtension as UriBound>::uri())
///         .unwrap()
///         .downcast_ref()
///         .unwrap();
///     unsafe { assert_eq!((interface.foo)(&my_plugin), 17.0) }
#[macro_export]
macro_rules! match_extensions {
    ($uri:expr, $($extension:ty),*) => {
        match ($uri).to_bytes_with_nul() {
            $(
                <$ extension as ::lv2_core::UriBound>::URI => Some(<$extension
                 as ::lv2_core::extension::Extension<Self>>::RAW_DATA),
            )*
            _ => None,
        }
    };
}