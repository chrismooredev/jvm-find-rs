
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ops::Deref;

#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("Error running Java executable on system path")]
	JavaExecution(#[source] std::io::Error),

	#[error("Error validating environment variable JAVA_HOME as existing directory")]
	ValidationError(#[source] std::io::Error),

	// Is this possible? (I guess technically yes, but do any JVM impls not?)
	#[error("The installed java executable did not report a `java.home` property")]
	NoJavaHomeProperty,

	#[error("Attempted to perform an operation with a non-utf8 path that does not support non-utf8 paths")]
	PathNotUTF8(PathBuf),

	#[error("Unspecified error while globbing JAVA_HOME")]
	GlobError(#[from] glob::GlobError),

	#[error("Unable to find native library file within JAVA_HOME")]
	NoNativeLibrary,
}

type Result<T> = std::result::Result<T, Error>;

pub const NATIVE_LIBRARY_FILENAME_WIN: &str = "jvm.dll";
pub const NATIVE_LIBRARY_FILENAME_LIN: &str = "libjvm.so";
pub const NATIVE_LIBRARY_FILENAME_MAC: &str = "libjli.dylib";

#[cfg(target_os = "windows")]
pub const NATIVE_LIBRARY_FILENAME: &str = NATIVE_LIBRARY_FILENAME_WIN;
#[cfg(any(
	target_os = "freebsd",
	target_os = "linux",
	target_os = "netbsd",
	target_os = "openbsd"
))]
pub const NATIVE_LIBRARY_FILENAME: &str = NATIVE_LIBRARY_FILENAME_LIN;
#[cfg(target_os = "macos")]
pub const NATIVE_LIBRARY_FILENAME: &str = NATIVE_LIBRARY_FILENAME_MAC;


#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JavaHome {
	pub path: PathBuf,
}
impl JavaHome {
	pub const ENV_VAR: &'static str = "JAVA_HOME";

	/// Returns the existing JAVA_HOME environment variable if it exists, or queries the active Java installation.
	pub fn find_home() -> Result<Self> {
		match std::env::var_os(JavaHome::ENV_VAR).map(PathBuf::from) {
			Some(path) if !path.as_os_str().is_empty() => Ok(JavaHome { path }),
			_ => JavaHome::find_active_home(),
		}
	}

	/// Checks any existing JAVA_HOME environment variable for a valid directory, and if not, it falls back to the currently active Java installation.
	pub fn find_valid_home() -> Result<Self> {
		match std::env::var_os(JavaHome::ENV_VAR).map(PathBuf::from) {
			Some(path) if !path.as_os_str().is_empty() && path.is_dir() => Ok(JavaHome { path }),
			_ => JavaHome::find_active_home(),
		}
	}

	/// Queries the first found `java` executable on the system path for its home directory.
	pub fn find_active_home() -> Result<Self> {
		// TODO: Query registry on windows?
		// https://docs.oracle.com/javase/9/install/installation-jdk-and-jre-microsoft-windows-platforms.htm#JSJIG-GUID-C11500A9-252C-46FE-BB17-FC5A9528EAEB

		let output = Command::new("java")
			.arg("-XshowSettings:properties")
			.arg("-version")
			.output()
			.map_err(|e| Error::JavaExecution(e))?;

		let stdout = String::from_utf8_lossy(&output.stdout);
		let stderr = String::from_utf8_lossy(&output.stderr);
		let java_home = stdout.lines()
			.chain(stderr.lines())
			.filter(|line| line.contains("java.home"))
			.find_map(|line| line.find('=').map(|i| line[i..].trim()));

		match java_home {
			Some(path) => Ok(JavaHome { path: PathBuf::from(path) }),
			None => Err(Error::NoJavaHomeProperty),
		}
	}

	// (will happily accept PRs for these)
	
	// all installations found on path (walk path and get the home dir for all java executables on it)
	// TODO: pub fn installations() -> Result<Vec<PathBuf>>

	// jre home
	// TODO: pub fn jre(&self) -> Result<PathBuf>
	
	// jdk home
	// TODO: pub fn jdk(&self) -> Result<Option<PathBuf>>

	// binary folder
	// TODO: pub fn bin(&self) -> Result<PathBuf>

	/// If the JDK is installed, returns a list of all the include folders necesssary to load JNI/etc headers.
	#[cfg(feature = "glob")]
	pub fn include(&self) -> Result<Option<Vec<PathBuf>>> {
		let base = self.join("include");

		Ok(if base.is_dir() {
			let escaped = glob::Pattern::escape(base.to_str().ok_or_else(|| Error::PathNotUTF8(base.clone()))?);
			let pattern = escaped + "/**/*/";
			Some(
				std::iter::once(Ok(base.clone())) // include base dir
					.chain(glob::glob(&pattern).unwrap()) // platform dependent dirs
					.collect::<std::result::Result<Vec<PathBuf>, glob::GlobError>>()? // collect + bubble errors
			)
		} else {
			None
		})
	}

	/// The path to the JVM's native library, suitable for linking with (jvm.dll/libjvm.so/libjli.dylib)
	#[cfg(feature = "glob")]
	pub fn native_library(&self) -> Result<PathBuf> {
		let base = &self.path;
		let escaped = glob::Pattern::escape(base.to_str().ok_or_else(|| Error::PathNotUTF8(base.clone()))?);
		let pattern = escaped + "/**/" + NATIVE_LIBRARY_FILENAME;
		Ok(glob::glob(&pattern)
			.unwrap() // pattern should always be valid
			.next().ok_or(Error::NoNativeLibrary)??)
	}
}
impl Deref for JavaHome {
	type Target = Path;
	fn deref(&self) -> &Self::Target {
		&self.path
	}
}
