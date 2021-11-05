
#![doc(html_root_url = "https://docs.rs/jvm-find/0.1.1")]

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ops::Deref;

// TODO: apply #[doc(cfg(feature = "glob"))] to applicable features when stabilized (#43781)

#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("Error running Java executable on system path")]
	JavaExecution(#[source] std::io::Error),

	#[error("Error accessing JavaHome path")]
	IoError(#[source] std::io::Error),

	#[error("The JavaHome path (possibly obtained from JAVA_HOME environment variable) contained bad data. It could have been outdated, or pointing to something other than a directory. (bad path: {})", .0.display())]
	BadJavaHomePath(PathBuf),

	// Is this possible? (I guess technically yes, but do any JVM impls not?)
	#[error("The installed java executable did not report a `java.home` property")]
	NoJavaHomeProperty,

	#[cfg(feature = "glob")]
	#[error("Attempted to perform an operation with a non-utf8 path that does not support non-utf8 paths")]
	PathNotUTF8(PathBuf),

	#[cfg(feature = "glob")]
	#[error("Unspecified error while globbing JAVA_HOME")]
	GlobError(#[from] glob::GlobError),

	#[cfg(feature = "glob")]
	#[error("Unable to find native library file within JAVA_HOME")]
	NoNativeLibrary,
}

type Result<T> = std::result::Result<T, Error>;

/// The Windows native library that applications can link to - `jvm.dll`
pub const NATIVE_LIBRARY_FILENAME_WIN: &str = "jvm.dll";
/// The Linux shared library object that applications can link to - `libjvm.so`
pub const NATIVE_LIBRARY_FILENAME_LIN: &str = "libjvm.so";
/// The MacOS dynamic library that applications can link to - `libjli.dylib`
///
/// Note that MacOS consumers should link to `libjli.dylib` instead of `libjvm.dylib` due to a bug with the distribution.
pub const NATIVE_LIBRARY_FILENAME_MAC: &str = "libjli.dylib";
// (does the MacOS native library need to depend on the version of Java?)

/// The specific native library filename, chosen for the built platform.
#[cfg(target_os = "windows")]
pub const NATIVE_LIBRARY_FILENAME: &str = NATIVE_LIBRARY_FILENAME_WIN;
/// The specific native library filename, chosen for the built platform.
#[cfg(any(
	target_os = "freebsd",
	target_os = "linux",
	target_os = "netbsd",
	target_os = "openbsd"
))]
pub const NATIVE_LIBRARY_FILENAME: &str = NATIVE_LIBRARY_FILENAME_LIN;
/// The specific native library filename, chosen for the built platform.
#[cfg(target_os = "macos")]
pub const NATIVE_LIBRARY_FILENAME: &str = NATIVE_LIBRARY_FILENAME_MAC;

/// A located Java home directory. Note that this type does not necessarily contain a valid one (in the case of a bad JAVA_HOME variable, custom-created one, etc) - as such, all usages of the contained path must be validated.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JavaHome {
	pub path: PathBuf,
}
impl JavaHome {
	/// The `JAVA_HOME` environment variable name.
	pub const ENV_VAR: &'static str = "JAVA_HOME";

	/// Returns the existing JAVA_HOME environment variable if it is non-empty, or queries the active Java installation.
	///
	/// # Errors
	/// This function will error in any situation that `JavaHome::find_active_home()` will - refer to that function for detailed error cases.
	pub fn find_home() -> Result<Self> {
		std::env::var_os(JavaHome::ENV_VAR)
			.filter(|var| !var.is_empty())
			.map(PathBuf::from)
			.map(|path| Ok(JavaHome { path }))
			.unwrap_or_else(JavaHome::find_active_home)
	}

	/// Checks that any existing JAVA_HOME environment variable points to a valid directory, and if not, it falls back to the currently active Java installation's home directory.
	///
	/// # Errors
	/// This will error if the directory specified by JAVA_HOME is unreachable (due to permissions/broken links/etc errors). Also contains error conditions as specified by `JavaHome::find_active_home()`
	pub fn find_valid_home() -> Result<Self> {
		std::env::var_os(JavaHome::ENV_VAR)
			.filter(|var| !var.is_empty())
			.map(PathBuf::from)
			.map(|path| {
				match path.metadata() {
					// bubble up IO (permission/etc) errors
					Err(e) if e.kind() != ErrorKind::NotFound => Some(Err(Error::IoError(e))),

					// return Some(pb) iff this is an existing directory
					Ok(meta) if meta.is_dir() => Some(Ok(JavaHome { path })),

					// It's not a directory? outdated env var? Query java executable
					_ => None
				}
			})
			.flatten().transpose()?.map(Ok)

			// fallback to JavaHome::find_active_home
			.unwrap_or_else(JavaHome::find_active_home)
	}

	/// Queries the first found `java` executable on the system path for its home directory.
	///
	/// # Errors
	/// This function will error if there is an issue finding/running a `java` executable from the path, or if `java -XshowSettings:properties -version` does not return a `java.home` property.
	pub fn find_active_home() -> Result<Self> {
		// TODO: Query registry on windows?
		// https://docs.oracle.com/javase/9/install/installation-jdk-and-jre-microsoft-windows-platforms.htm#JSJIG-GUID-C11500A9-252C-46FE-BB17-FC5A9528EAEB

		log::debug!("finding currently active JAVA_HOME location by running the `java` command from the system path");

		let output = Command::new("java")
			.arg("-XshowSettings:properties")
			.arg("-version")
			.output()
			.map_err(|e| Error::JavaExecution(e))?;

		let stdout = String::from_utf8_lossy(&output.stdout);
		let stderr = String::from_utf8_lossy(&output.stderr);
		let java_home_raw = stdout.lines()
			.chain(stderr.lines())
			.find(|line| line.contains("java.home"));
	
		match &java_home_raw {
			Some(l) => log::debug!("\tfound: {}", l),
			None => log::debug!("\tnot found"),
		};

		let java_home = java_home_raw.map(|line| line.find('=').map(|i| line[i+1..].trim()));

		match java_home.flatten() {
			Some(path) => Ok(JavaHome { path: PathBuf::from(path) }),
			None => Err(Error::NoJavaHomeProperty),
		}
	}

	// (will happily accept PRs for these)
	
	// All installations found on path (walk path and get the home dir for all java executables on it)
	// (query registry if on Windows?)
	//   HKLM/SOFTWARE/JavaSoft/Java Development Kit/(<= JDK 1.8)/JavaHome
	//   HKLM/SOFTWARE/JavaSoft/JDK/(>= JDK 1.9)/JavaHome
	//   HKLM/SOFTWARE/JavaSoft/Java Runtime Environment/(<= JDK 1.8)/JavaHome
	//   HKLM/SOFTWARE/JavaSoft/JRE/(>= JDK 1.9)/JavaHome
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
		log::debug!("looking for $JAVA_HOME/include at {:?}", base);

		match base.metadata() {
			Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
			Err(e) => Err(Error::IoError(e)),
			Ok(meta) if meta.is_dir() => Ok(Some({
				let escaped = glob::Pattern::escape(base.to_str().ok_or_else(|| Error::PathNotUTF8(base.clone()))?);
				let pattern = escaped + "/**/*/";

				std::iter::once(Ok(base.clone())) // include base dir
					.chain(glob::glob(&pattern).unwrap()) // platform dependent dirs
					.collect::<std::result::Result<Vec<PathBuf>, glob::GlobError>>()? // collect + bubble errors
			})),
			Ok(_) => Err(Error::BadJavaHomePath(self.path.clone())),
		}
	}

	/// The path to the JVM's platform-specific native library, suitable for linking with. (`jvm.dll`/`libjvm.so`/`libjli.dylib`)
	#[cfg(feature = "glob")]
	pub fn native_library(&self) -> Result<PathBuf> {
		let base = &self.path;
		let escaped = glob::Pattern::escape(base.to_str().ok_or_else(|| Error::PathNotUTF8(base.clone()))?);
		let pattern = escaped + "/**/" + NATIVE_LIBRARY_FILENAME;
		log::debug!("looking for JVM native library with glob {:?}", pattern);
		// developer note: if on linux, LD_LIBRARY_PATH may need to be set for the system loader to find it
		// alternatively, is there a way to tell cargo to use the absolute path?

		Ok(glob::glob(&pattern)
			.unwrap() // pattern should always be valid
			.next().ok_or(Error::NoNativeLibrary)??)
	}

	/// A convience function to search for a specific file within the home directory. Matches the filename literally.
	#[cfg(feature = "glob")]
	pub fn find_file(&self, file: &str) -> Result<Option<PathBuf>> {
		let base = &self.path;
		let base_escaped = glob::Pattern::escape(base.to_str().ok_or_else(|| Error::PathNotUTF8(base.clone()))?);
		let file_escaped = glob::Pattern::escape(file);
		let pattern = base_escaped + "/**/" + &file_escaped;
		glob::glob(&pattern)
			.unwrap() // pattern should always be valid
			.next()
			.transpose()
			.map_err(Error::GlobError)
	}

	/// A convience function to search for a specific folder within the home directory. Matches the folder name literally.
	#[cfg(feature = "glob")]
	pub fn find_folder(&self, folder: &str) -> Result<Option<PathBuf>> {
		let base = &self.path;
		let base_escaped = glob::Pattern::escape(base.to_str().ok_or_else(|| Error::PathNotUTF8(base.clone()))?);
		let fold_escaped = glob::Pattern::escape(folder);
		let pattern = base_escaped + "/**/" + &fold_escaped + "/";
		glob::glob(&pattern)
			.unwrap() // pattern should always be valid
			.next()
			.transpose()
			.map_err(Error::GlobError)
	}
}
impl Deref for JavaHome {
	type Target = Path;
	fn deref(&self) -> &Self::Target {
		&self.path
	}
}
