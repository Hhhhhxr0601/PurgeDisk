mod info;
mod platform;

pub use info::Device;
pub use platform::list_removable_devices;
pub use platform::open_raw_device;
pub use platform::reformat_device;
