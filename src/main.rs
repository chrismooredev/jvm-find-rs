
use jvm_find::JavaHome;

fn main() -> Result<(), jvm_find::Error> {
	println!("find_home(): {}", JavaHome::find_home()?.display());
	println!("find_active_home(): {}", JavaHome::find_active_home()?.display());
	println!("find_valid_home(): {}", JavaHome::find_valid_home()?.display());

	let home = JavaHome::find_home().unwrap();

	if let Some(includes) = home.include()? {
		println!("include:");
		for inc in includes {
			println!("\t{}", inc.display());
		}
	} else {
		println!("include: <JDK not installed?>");
	}
	println!("native library: {}", home.native_library()?.display());

	Ok(())
}
