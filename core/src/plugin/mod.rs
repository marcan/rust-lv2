//! Types to create plugins.
pub(crate) mod info;
pub mod port;

pub use info::PluginInfo;
pub use lv2_core_derive::*;

use crate::feature::*;
use std::any::Any;
use std::ffi::{c_void, CStr};
use std::os::raw::c_char;
use sys::LV2_Handle;

/// Container for port handling.
///
/// Plugins do not handle port management on their own. Instead, they define a struct with all of the required ports. Then, the plugin instance will collect the port pointers from the host and create a `PortContainer` instance for every `run` call. Using this instance, plugins have access to all of their required ports.
///
/// # Implementing
///
/// The most convenient way to create port containers is to define a struct with port types from the [`port`](port/index.html) module and then simply derive `PortContainer` for it. An example:
///
///     use lv2_core::plugin::PortContainer;
///     use lv2_core::plugin::port::*;
///
///     #[derive(PortContainer)]
///     struct MyPortContainer {
///         audio_input: InputPort<Audio>,
///         audio_output: OutputPort<Audio>,
///         control_input: InputPort<Control>,
///         control_output: OutputPort<Control>,
///         optional_control_input: Option<InputPort<Control>>,
///     }
///
/// Please note that port indices are mapped in the order of occurence; In our example, the implementation will treat `audio_input` as port `0`, `audio_output` as port `1` and so on. Therefore, your plugin definition and your port container have to match. Otherwise, undefined behaviour will occur.
pub trait PortContainer: Sized {
    /// The type of the port pointer cache.
    ///
    /// The host passes port pointers to the plugin one by one and in an undefined order. Therefore, the plugin instance can not collect these pointers in the port container directly. Instead, the pointers are stored in a cache which is then used to create the proper port container.
    type Cache: PortPointerCache;

    /// Try to construct a port container instance from a port pointer cache.
    ///
    /// If one of the port connection pointers is null, this method will return `None`, because a `PortContainer` can not be constructed.
    ///
    /// # unsafety
    ///
    /// Since the pointer cache is only storing the pointers, implementing this method requires the de-referencation of raw pointers and therefore, this method is unsafe.
    unsafe fn from_connections(cache: &Self::Cache, sample_count: u32) -> Option<Self>;
}

impl PortContainer for () {
    type Cache = ();

    unsafe fn from_connections(_cache: &(), _sample_count: u32) -> Option<Self> {
        Some(())
    }
}

/// Cache for port connection pointers.
///
/// The host will pass the port connection pointers one by one and in an undefined order. Therefore, the `PortContainer` struct can not be created instantly. Instead, the pointers will be stored in a cache, which is then used to create a proper port container for the plugin.
pub trait PortPointerCache: Sized + Default {
    /// Store the connection pointer for the port with index `index`.
    ///
    /// The passed pointer may not be valid yet and therefore, implementors should only store the pointer, not dereference it.
    fn connect(&mut self, index: u32, pointer: *mut c_void);
}

impl PortPointerCache for () {
    fn connect(&mut self, _index: u32, _pointer: *mut c_void) {}
}

/// The central trait to describe LV2 plugins.
///
/// This trait and the structs that implement it are the centre of every plugin project, since it hosts the `run` method. This method is called by the host for every processing cycle.
///
/// However, the host will not directly talk to the plugin. Instead, it will create and talk to the [`PluginInstance`](struct.PluginInstance.html), which dereferences raw pointers, does safety checks and then calls the corresponding plugin methods.
pub trait Plugin: Sized + Send + Sync + 'static {
    /// The type of the port container.
    type Ports: PortContainer;

    /// Create a new plugin instance.
    ///
    /// This method only creates an instance of the plugin, it does not reset or set up it's internal state. This is done by the `activate` method.
    fn new(plugin_info: &PluginInfo, features: FeatureContainer) -> Self;

    /// Run a processing step.
    ///
    /// The host will always call this method after `active` has been called and before `deactivate` has been called.
    fn run(&mut self, ports: &mut Self::Ports);

    /// Reset and initialize the complete internal state of the plugin.
    ///
    /// This method will be called if the plugin has just been created of if the plugin has been deactivated. Also, a host's `activate` call will be as close as possible to the first `run` call.
    fn activate(&mut self) {}

    /// Deactivate the plugin.
    ///
    /// The host will always call this method when it wants to shut the plugin down. After `deactivate` has been called, `run` will not be called until `activate` has been called again.
    fn deactivate(&mut self) {}

    /// Return additional, extension-specific data.
    ///
    /// Sometimes, the methods from the `Plugin` trait aren't enough to support additional LV2 specifications. For these cases, extension exist. In most cases and for Rust users, an extension is simply a trait that can be implemented for a plugin.
    ///
    /// However, these implemented methods must be passed to the host. This is where this method comes into play: The host will call it with a URI for an extension. Then, it is the plugin's responsibilty to return the extension data to the host.
    ///
    /// In most cases, you can simply use the [`match_extensions`](../macro.match_extensions.html) macro to generate an appropiate method body.
    fn extension_data(_uri: &CStr) -> Option<&'static dyn Any> {
        None
    }
}

/// Plugin wrapper which translated between the host and the plugin.
///
/// The host interacts with the plugin via a C API, but the plugin is implemented with ideomatic, safe Rust. To bridge this gap, this wrapper is used to translate and abstract the communcation between the host and the plugin.
pub struct PluginInstance<T: Plugin> {
    instance: T,
    connections: <T::Ports as PortContainer>::Cache,
}

impl<T: Plugin> PluginInstance<T> {
    /// Instantiate the plugin.
    pub unsafe extern "C" fn instantiate(
        descriptor: *const sys::LV2_Descriptor,
        sample_rate: f64,
        bundle_path: *const c_char,
        features: *const *const sys::LV2_Feature,
    ) -> LV2_Handle {
        // Dereference the descriptor.
        let descriptor = match descriptor.as_ref() {
            Some(descriptor) => descriptor,
            None => {
                eprintln!("Failed to initialize plugin: Descriptor points to null");
                return std::ptr::null_mut();
            }
        };

        // Dereference the plugin info.
        let plugin_info = match PluginInfo::from_raw(descriptor, bundle_path, sample_rate) {
            Ok(info) => info,
            Err(e) => {
                eprintln!(
                    "Failed to initialize plugin: Illegal info from host: {:?}",
                    e
                );
                return std::ptr::null_mut();
            }
        };

        // Collect the supported features.
        let features = FeatureContainer::from_raw(features);

        // Instantiate the plugin.
        let instance = Box::new(Self {
            instance: T::new(&plugin_info, features),
            connections: <<T::Ports as PortContainer>::Cache as Default>::default(),
        });
        Box::leak(instance) as *mut Self as LV2_Handle
    }

    /// Clean the plugin.
    pub unsafe extern "C" fn cleanup(instance: *mut c_void) {
        let instance = instance as *mut Self;
        Box::from_raw(instance);
    }

    /// Call `activate`.
    pub unsafe extern "C" fn activate(instance: *mut c_void) {
        let instance = instance as *mut Self;
        (*instance).instance.activate()
    }

    /// Call `deactivate`
    pub unsafe extern "C" fn deactivate(instance: *mut c_void) {
        let instance = instance as *mut Self;
        (*instance).instance.deactivate()
    }

    /// Update a port pointer.
    pub unsafe extern "C" fn connect_port(instance: *mut c_void, port: u32, data: *mut c_void) {
        let instance = instance as *mut Self;
        (*instance).connections.connect(port, data)
    }

    /// Construct a port container and call the `run` method.
    pub unsafe extern "C" fn run(instance: *mut c_void, sample_count: u32) {
        let instance = instance as *mut Self;
        let ports =
            <T::Ports as PortContainer>::from_connections(&(*instance).connections, sample_count);
        if let Some(mut ports) = ports {
            (*instance).instance.run(&mut ports);
        }
    }

    /// Dereference the URI, call the `extension_data` function and return the pointer.
    pub unsafe extern "C" fn extension_data(uri: *const c_char) -> *const c_void {
        let uri = CStr::from_ptr(uri);
        if let Some(data) = T::extension_data(uri) {
            data as *const _ as *const c_void
        } else {
            std::ptr::null()
        }
    }
}

#[doc(hidden)]
pub unsafe trait PluginInstanceDescriptor: Plugin {
    const URI: &'static [u8];
    const DESCRIPTOR: sys::LV2_Descriptor;
}
