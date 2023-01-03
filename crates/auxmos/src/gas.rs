#[allow(dead_code)]
pub mod constants;

pub mod mixture;

pub mod types;

use auxtools::*;

pub use types::*;

use fxhash::FxBuildHasher;

use parking_lot::{const_rwlock, RwLock};

pub use mixture::Mixture;

use std::{cell::RefCell, collections::HashSet};

pub type GasIDX = usize;

use once_cell::sync::Lazy;

/// A static container, with a bunch of helper functions for accessing global data. It's horrible, I know, but video games.
pub struct GasArena {}

/*
	This is where the gases live.
	This is just a big vector, acting as a gas mixture pool.
	As you can see, it can be accessed by any thread at any time;
	of course, it has a RwLock preventing this, and you can't access the
	vector directly. Seriously, please don't. I have the wrapper functions for a reason.
*/
static GAS_MIXTURES: RwLock<Option<Vec<RwLock<Mixture>>>> = const_rwlock(None);

static NEXT_GAS_IDS: Lazy<(crossbeam_channel::Sender<usize>, crossbeam_channel::Receiver<usize>)> = Lazy::new(|| crossbeam_channel::bounded(2000));

thread_local! {
	static REGISTERED_GAS_MIXES: RefCell<Option<HashSet<u32, FxBuildHasher>>> = RefCell::new(None);
}

//is registered mix may be called when byond's del datum runs after world shutdown is done.
//this is allowed to fail because of that
fn is_registered_mix(i: u32) -> bool {
	REGISTERED_GAS_MIXES.with(|thin| {
		thin.borrow()
			.as_ref()
			.map(|opt| opt.contains(&i))
			.unwrap_or(false)
	})
}

fn register_mix(v: &Value) {
	REGISTERED_GAS_MIXES.with(|thin| {
		thin.borrow_mut()
			.as_mut()
			.expect("Wrong thread tried to access REGISTERED_GAS_MIXES, must be the main thread!")
			.insert(unsafe { v.raw.data.id })
	});
}

//Unregister mix may be called when byond's del datum runs after world shutdown is done.
//this is allowed to fail because of that
fn unregister_mix(i: u32) {
	REGISTERED_GAS_MIXES.with(|thin| {
		thin.borrow_mut().as_mut().map(|opt| opt.remove(&i));
	});
}

#[init(partial)]
fn _initialize_gas_mixtures() -> Result<(), String> {
	*GAS_MIXTURES.write() = Some(Vec::with_capacity(240_000));
	REGISTERED_GAS_MIXES.with(|thing| *thing.borrow_mut() = Some(Default::default()));
	Ok(())
}

#[shutdown]
fn _shut_down_gases() {
	crate::turfs::wait_for_tasks();
	GAS_MIXTURES.write().as_mut().unwrap().clear();
	while !NEXT_GAS_IDS.1.is_empty() {
		NEXT_GAS_IDS.1.recv().unwrap();
	}
	REGISTERED_GAS_MIXES.with(|thing| *thing.borrow_mut() = None);
}

impl GasArena {
	/// Locks the gas arena and and runs the given closure with it locked.
	/// # Panics
	/// if `GAS_MIXTURES` hasn't been initialized, somehow.
	pub fn with_all_mixtures<T, F>(f: F) -> T
	where
		F: FnOnce(&[RwLock<Mixture>]) -> T,
	{
		f(GAS_MIXTURES.read().as_ref().unwrap())
	}
	/// Read locks the given gas mixture and runs the given closure on it.
	/// # Errors
	/// If no such gas mixture exists or the closure itself errors.
	/// # Panics
	/// if `GAS_MIXTURES` hasn't been initialized, somehow.
	pub fn with_gas_mixture<T, F>(id: usize, f: F) -> Result<T, Runtime>
	where
		F: FnOnce(&Mixture) -> Result<T, Runtime>,
	{
		let lock = GAS_MIXTURES.read();
		let gas_mixtures = lock.as_ref().unwrap();
		let mix = gas_mixtures
			.get(id)
			.ok_or_else(|| runtime!("No gas mixture with ID {} exists!", id))?
			.read();
		f(&mix)
	}
	/// Write locks the given gas mixture and runs the given closure on it.
	/// # Errors
	/// If no such gas mixture exists or the closure itself errors.
	/// # Panics
	/// if `GAS_MIXTURES` hasn't been initialized, somehow.
	pub fn with_gas_mixture_mut<T, F>(id: usize, f: F) -> Result<T, Runtime>
	where
		F: FnOnce(&mut Mixture) -> Result<T, Runtime>,
	{
		let lock = GAS_MIXTURES.read();
		let gas_mixtures = lock.as_ref().unwrap();
		let mut mix = gas_mixtures
			.get(id)
			.ok_or_else(|| runtime!("No gas mixture with ID {} exists!", id))?
			.write();
		f(&mut mix)
	}
	/// Read locks the given gas mixtures and runs the given closure on them.
	/// # Errors
	/// If no such gas mixture exists or the closure itself errors.
	/// # Panics
	/// if `GAS_MIXTURES` hasn't been initialized, somehow.
	pub fn with_gas_mixtures<T, F>(src: usize, arg: usize, f: F) -> Result<T, Runtime>
	where
		F: FnOnce(&Mixture, &Mixture) -> Result<T, Runtime>,
	{
		let lock = GAS_MIXTURES.read();
		let gas_mixtures = lock.as_ref().unwrap();
		let src_gas = gas_mixtures
			.get(src)
			.ok_or_else(|| runtime!("No gas mixture with ID {} exists!", src))?
			.read();
		let arg_gas = gas_mixtures
			.get(arg)
			.ok_or_else(|| runtime!("No gas mixture with ID {} exists!", arg))?
			.read();
		f(&src_gas, &arg_gas)
	}
	/// Locks the given gas mixtures and runs the given closure on them.
	/// # Errors
	/// If no such gas mixture exists or the closure itself errors.
	/// # Panics
	/// if `GAS_MIXTURES` hasn't been initialized, somehow.
	pub fn with_gas_mixtures_mut<T, F>(src: usize, arg: usize, f: F) -> Result<T, Runtime>
	where
		F: FnOnce(&mut Mixture, &mut Mixture) -> Result<T, Runtime>,
	{
		let src = src;
		let arg = arg;
		let lock = GAS_MIXTURES.read();
		let gas_mixtures = lock.as_ref().unwrap();
		if src == arg {
			let mut entry = gas_mixtures
				.get(src)
				.ok_or_else(|| runtime!("No gas mixture with ID {} exists!", src))?
				.write();
			let mix = &mut entry;
			let mut copied = mix.clone();
			f(mix, &mut copied)
		} else {
			f(
				&mut gas_mixtures
					.get(src)
					.ok_or_else(|| runtime!("No gas mixture with ID {} exists!", src))?
					.write(),
				&mut gas_mixtures
					.get(arg)
					.ok_or_else(|| runtime!("No gas mixture with ID {} exists!", arg))?
					.write(),
			)
		}
	}
	/// Runs the given closure on the gas mixture *locks* rather than an already-locked version.
	/// # Errors
	/// If no such gas mixture exists or the closure itself errors.
	/// # Panics
	/// if `GAS_MIXTURES` hasn't been initialized, somehow.
	fn with_gas_mixtures_custom<T, F>(src: usize, arg: usize, f: F) -> Result<T, Runtime>
	where
		F: FnOnce(&RwLock<Mixture>, &RwLock<Mixture>) -> Result<T, Runtime>,
	{
		let src = src;
		let arg = arg;
		let lock = GAS_MIXTURES.read();
		let gas_mixtures = lock.as_ref().unwrap();
		if src == arg {
			let entry = gas_mixtures
				.get(src)
				.ok_or_else(|| runtime!("No gas mixture with ID {} exists!", src))?;
			let gas_copy = entry.read().clone();
			f(entry, &RwLock::new(gas_copy))
		} else {
			f(
				gas_mixtures
					.get(src)
					.ok_or_else(|| runtime!("No gas mixture with ID {} exists!", src))?,
				gas_mixtures
					.get(arg)
					.ok_or_else(|| runtime!("No gas mixture with ID {} exists!", arg))?,
			)
		}
	}
	/// Fills in the first unused slot in the gas mixtures vector, or adds another one, then sets the argument Value to point to it.
	/// # Errors
	/// If `initial_volume` is incorrect or `_extools_pointer_gasmixture` doesn't exist, somehow.
	/// # Panics
	/// If not called from the main thread
	/// If `NEXT_GAS_IDS` is not initialized, somehow.
	pub fn register_mix(mix: &Value) -> DMResult {
		if NEXT_GAS_IDS.1.is_empty() {
			let mut lock = GAS_MIXTURES.write();
			let gas_mixtures = lock.as_mut().unwrap();
			let next_idx = gas_mixtures.len();
			gas_mixtures.push(RwLock::new(Mixture::from_vol(
				mix.get_number(byond_string!("initial_volume"))
					.map_err(|_| {
						runtime!(
							"Attempt to interpret non-number value as number {} {}:{}",
							std::file!(),
							std::line!(),
							std::column!()
						)
					})?,
			)));
			mix.set(
				byond_string!("_extools_pointer_gasmixture"),
				f32::from_bits(next_idx as u32),
			)?;
		} else {
			let idx = {
				NEXT_GAS_IDS.1.recv().unwrap()
			};
			GAS_MIXTURES
				.read()
				.as_ref()
				.unwrap()
				.get(idx)
				.unwrap()
				.write()
				.clear_with_vol(
					mix.get_number(byond_string!("initial_volume"))
						.map_err(|_| {
							runtime!(
								"Attempt to interpret non-number value as number {} {}:{}",
								std::file!(),
								std::line!(),
								std::column!()
							)
						})?,
				);
			mix.set(
				byond_string!("_extools_pointer_gasmixture"),
				f32::from_bits(idx as u32),
			)?;
		}
		register_mix(mix);
		rayon::spawn(|| {
			if NEXT_GAS_IDS.1.is_empty() {
				let mut gas_lock = GAS_MIXTURES.write();
				let gas_mixtures = gas_lock.as_mut().unwrap();
				let cur_last = gas_mixtures.len();
				let cap = {
					let to_cap = gas_mixtures.capacity() - cur_last;
					if to_cap == 0 {
						NEXT_GAS_IDS.1.capacity().unwrap() - 100
					} else {
						(NEXT_GAS_IDS.1.capacity().unwrap() - 100).min(to_cap)
					}
				};
				for i in cur_last..(cur_last + cap) {
					NEXT_GAS_IDS.0.send(i).unwrap();
				}
				gas_mixtures.resize_with(cur_last + cap, Default::default);
			}
		});
		Ok(Value::null())
	}
	/// Marks the Value's gas mixture as unused, allowing it to be reallocated to another.
	/// # Panics
	/// If not called from the main thread
	/// If `NEXT_GAS_IDS` hasn't been initialized, somehow.
	pub fn unregister_mix(mix: u32) {
		if is_registered_mix(mix) {
			use raw_types::values::{ValueData, ValueTag};
			unsafe {
				let mut raw = raw_types::values::Value {
					tag: ValueTag::Null,
					data: ValueData { id: 0 },
				};
				let this_mix = raw_types::values::Value {
					tag: ValueTag::Datum,
					data: ValueData { id: mix },
				};
				let err = raw_types::funcs::get_variable(
					&mut raw,
					this_mix,
					byond_string!("_extools_pointer_gasmixture").get_id(),
				);
				if err == 1 {
					let idx = raw.data.number.to_bits();
					{
						NEXT_GAS_IDS.0.send(idx as usize).unwrap();
					}
					unregister_mix(mix);
				}
			}
		}
	}
}

/// Gets the mix for the given value, and calls the provided closure with a reference to that mix as an argument.
/// # Errors
/// If a gasmixture ID is not a number or the callback returns an error.
pub fn with_mix<T, F>(mix: &Value, f: F) -> Result<T, Runtime>
where
	F: FnMut(&Mixture) -> Result<T, Runtime>,
{
	GasArena::with_gas_mixture(
		mix.get_number(byond_string!("_extools_pointer_gasmixture"))
			.map_err(|_| {
				runtime!(
					"Attempt to interpret non-number value as number {} {}:{}",
					std::file!(),
					std::line!(),
					std::column!()
				)
			})?
			.to_bits() as usize,
		f,
	)
}

/// As `with_mix`, but mutable.
/// # Errors
/// If a gasmixture ID is not a number or the callback returns an error.
pub fn with_mix_mut<T, F>(mix: &Value, f: F) -> Result<T, Runtime>
where
	F: FnMut(&mut Mixture) -> Result<T, Runtime>,
{
	GasArena::with_gas_mixture_mut(
		mix.get_number(byond_string!("_extools_pointer_gasmixture"))
			.map_err(|_| {
				runtime!(
					"Attempt to interpret non-number value as number {} {}:{}",
					std::file!(),
					std::line!(),
					std::column!()
				)
			})?
			.to_bits() as usize,
		f,
	)
}

/// As `with_mix`, but with two mixes.
/// # Errors
/// If a gasmixture ID is not a number or the callback returns an error.
pub fn with_mixes<T, F>(src_mix: &Value, arg_mix: &Value, f: F) -> Result<T, Runtime>
where
	F: FnMut(&Mixture, &Mixture) -> Result<T, Runtime>,
{
	GasArena::with_gas_mixtures(
		src_mix
			.get_number(byond_string!("_extools_pointer_gasmixture"))
			.map_err(|_| {
				runtime!(
					"Attempt to interpret non-number value as number {} {}:{}",
					std::file!(),
					std::line!(),
					std::column!()
				)
			})?
			.to_bits() as usize,
		arg_mix
			.get_number(byond_string!("_extools_pointer_gasmixture"))
			.map_err(|_| {
				runtime!(
					"Attempt to interpret non-number value as number {} {}:{}",
					std::file!(),
					std::line!(),
					std::column!()
				)
			})?
			.to_bits() as usize,
		f,
	)
}

/// As `with_mix_mut`, but with two mixes.
/// # Errors
/// If a gasmixture ID is not a number or the callback returns an error.
pub fn with_mixes_mut<T, F>(src_mix: &Value, arg_mix: &Value, f: F) -> Result<T, Runtime>
where
	F: FnMut(&mut Mixture, &mut Mixture) -> Result<T, Runtime>,
{
	GasArena::with_gas_mixtures_mut(
		src_mix
			.get_number(byond_string!("_extools_pointer_gasmixture"))
			.map_err(|_| {
				runtime!(
					"Attempt to interpret non-number value as number {} {}:{}",
					std::file!(),
					std::line!(),
					std::column!()
				)
			})?
			.to_bits() as usize,
		arg_mix
			.get_number(byond_string!("_extools_pointer_gasmixture"))
			.map_err(|_| {
				runtime!(
					"Attempt to interpret non-number value as number {} {}:{}",
					std::file!(),
					std::line!(),
					std::column!()
				)
			})?
			.to_bits() as usize,
		f,
	)
}

/// Allows different lock levels for each gas. Instead of relevant refs to the gases, returns the `RWLock` object.
/// # Errors
/// If a gasmixture ID is not a number or the callback returns an error.
pub fn with_mixes_custom<T, F>(src_mix: &Value, arg_mix: &Value, f: F) -> Result<T, Runtime>
where
	F: FnMut(&RwLock<Mixture>, &RwLock<Mixture>) -> Result<T, Runtime>,
{
	GasArena::with_gas_mixtures_custom(
		src_mix
			.get_number(byond_string!("_extools_pointer_gasmixture"))
			.map_err(|_| {
				runtime!(
					"Attempt to interpret non-number value as number {} {}:{}",
					std::file!(),
					std::line!(),
					std::column!()
				)
			})?
			.to_bits() as usize,
		arg_mix
			.get_number(byond_string!("_extools_pointer_gasmixture"))
			.map_err(|_| {
				runtime!(
					"Attempt to interpret non-number value as number {} {}:{}",
					std::file!(),
					std::line!(),
					std::column!()
				)
			})?
			.to_bits() as usize,
		f,
	)
}

pub fn amt_gases() -> usize {
	GAS_MIXTURES.read().as_ref().unwrap().len() - NEXT_GAS_IDS.1.len()
}

pub fn tot_gases() -> usize {
	GAS_MIXTURES.read().as_ref().unwrap().len()
}
