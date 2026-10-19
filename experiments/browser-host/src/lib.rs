#[cfg(all(target_os = "windows", feature = "sandbox"))]
pub mod shared;
#[cfg(all(target_os = "windows", feature = "sandbox"))]
mod win;

#[cfg(all(target_os = "windows", feature = "sandbox"))]
mod bridge;
