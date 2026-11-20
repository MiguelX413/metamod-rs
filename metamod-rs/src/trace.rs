use crate::{__ffi, Context, Server, engine::Engine};
use metamod_sys::raw::{self, edict_t};
use std::{cell::Cell, fmt, ptr};

thread_local! {
    // Keep the counter across deactivation so a new map cannot revive handles.
    static MAP: Cell<(u64, bool)> = const { Cell::new((0, false)) };
}

pub(crate) fn invalidate_map() {
    let (generation, _) = MAP.get();
    MAP.set((generation, false));
}

pub(crate) fn activate_map() {
    let (generation, _) = MAP.get();
    if let Some(generation) = generation.checked_add(1) {
        MAP.set((generation, true));
    } else {
        invalidate_map();
    }
}

fn active_map() -> Option<u64> {
    let (generation, active) = MAP.get();
    (active && __ffi::engine_available()).then_some(generation)
}

/// Identifies a live edict allocation within one map. The world edict
/// can have index zero. Keep the entire handle for identity comparisons.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct EntityHandle {
    index: i32,
    serial: i32,
    map: u64,
}

impl EntityHandle {
    /// Diagnostic engine index. It can be reused after entity removal.
    pub fn index(self) -> i32 {
        self.index
    }

    fn resolve(self, engine: Engine) -> Option<*mut edict_t> {
        let entity = engine.entity?;
        if active_map() != Some(self.map) {
            return None;
        }
        // SAFETY: The index originated from this map's engine edict table.
        let edict = unsafe { entity(self.index) };
        if active_map() != Some(self.map) {
            return None;
        }
        // SAFETY: The getter returned a current-map edict or null.
        (unsafe { live_serial(edict) } == Some(self.serial)).then_some(edict)
    }
}

/// A non-null edict must be readable until another native call or map change.
unsafe fn live_serial(edict: *mut edict_t) -> Option<i32> {
    if edict.is_null() {
        return None;
    }
    // SAFETY: These integer fields are initialized in the engine's edict.
    unsafe { ((*edict).free == 0).then(|| (*edict).serialnumber) }
}

impl Context<'_> {
    /// Revalidate the map and native allocation generation of a saved handle.
    pub fn is_entity_current(&self, entity: EntityHandle) -> bool {
        __ffi::engine()
            .and_then(|engine| entity.resolve(engine))
            .is_some()
    }
}

/// Whether the engine includes monsters in collision checks.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TraceMonsters {
    #[default]
    Include,
    Ignore,
}

/// The four standard GoldSrc hull numbers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum Hull {
    Point = raw::point_hull as i32,
    Human = raw::human_hull as i32,
    Large = raw::large_hull as i32,
    Head = raw::head_hull as i32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TraceOptions {
    pub monsters: TraceMonsters,
    pub skip: Option<EntityHandle>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TracePlane {
    pub normal: [f32; 3],
    pub distance: f32,
}

/// Owned trace output. An absent hit handle can also mean the native hit
/// became invalid during resolution. Use the fraction to detect a collision.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraceResult {
    pub all_solid: bool,
    pub start_solid: bool,
    pub in_open: bool,
    pub in_water: bool,
    pub fraction: f32,
    pub end_position: [f32; 3],
    pub plane: Option<TracePlane>,
    pub hit: Option<EntityHandle>,
    pub hit_group: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceError {
    Unavailable,
    InvalidCoordinates,
    StaleEntity,
    MapChanged,
}

impl fmt::Display for TraceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "tracing requires an active map and engine callbacks",
            Self::InvalidCoordinates => "trace coordinates must be finite",
            Self::StaleEntity => "the entity to skip is stale",
            Self::MapChanged => "the map changed during the trace",
        })
    }
}

impl std::error::Error for TraceError {}

impl Server<'_, '_> {
    pub fn trace_line(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        options: TraceOptions,
    ) -> Result<TraceResult, TraceError> {
        trace(start, end, options, None)
    }

    pub fn trace_hull(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        hull: Hull,
        options: TraceOptions,
    ) -> Result<TraceResult, TraceError> {
        trace(start, end, options, Some(hull))
    }
}

fn trace(
    start: [f32; 3],
    end: [f32; 3],
    options: TraceOptions,
    hull: Option<Hull>,
) -> Result<TraceResult, TraceError> {
    if !start.into_iter().chain(end).all(f32::is_finite) {
        return Err(TraceError::InvalidCoordinates);
    }
    let engine = __ffi::engine().ok_or(TraceError::Unavailable)?;
    let map = active_map().ok_or(TraceError::Unavailable)?;
    let skip = match options.skip {
        Some(handle) => handle.resolve(engine).ok_or(TraceError::StaleEntity)?,
        None => ptr::null_mut(),
    };
    let monsters = match options.monsters {
        TraceMonsters::Include => raw::dont_ignore_monsters as i32,
        TraceMonsters::Ignore => raw::ignore_monsters as i32,
    };
    let mut output = raw::TraceResult {
        flFraction: 1.0,
        ..Default::default()
    };
    // SAFETY: The vectors and output live through the synchronous call. The
    // skip edict, if present, was just resolved with no intervening native call.
    unsafe {
        match hull {
            Some(hull) => engine.trace_hull.ok_or(TraceError::Unavailable)?(
                start.as_ptr(),
                end.as_ptr(),
                monsters,
                hull as i32,
                skip,
                &mut output,
            ),
            None => engine.trace_line.ok_or(TraceError::Unavailable)?(
                start.as_ptr(),
                end.as_ptr(),
                monsters,
                skip,
                &mut output,
            ),
        }
    }
    if active_map() != Some(map) {
        return Err(TraceError::MapChanged);
    }
    let hit = capture_hit(engine, map, output.pHit);
    if active_map() != Some(map) {
        return Err(TraceError::MapChanged);
    }
    let vector = |v: raw::Vector| [v.x, v.y, v.z];
    Ok(TraceResult {
        all_solid: output.fAllSolid != 0,
        start_solid: output.fStartSolid != 0,
        in_open: output.fInOpen != 0,
        in_water: output.fInWater != 0,
        fraction: output.flFraction,
        end_position: vector(output.vecEndPos),
        plane: (output.fAllSolid == 0 && output.flFraction < 1.0).then_some(TracePlane {
            normal: vector(output.vecPlaneNormal),
            distance: output.flPlaneDist,
        }),
        hit,
        hit_group: output.iHitgroup,
    })
}

fn capture_hit(engine: Engine, map: u64, edict: *mut edict_t) -> Option<EntityHandle> {
    let index_of = engine.index?;
    // SAFETY: The trace returned a hit edict in the still-active map or null.
    let serial = unsafe { live_serial(edict) }?;
    // SAFETY: No native call intervened since the trace returned this edict.
    let index = unsafe { index_of(edict) };
    if index < 0 {
        return None;
    }
    let handle = EntityHandle { index, serial, map };
    (handle.resolve(engine) == Some(edict)).then_some(handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_reject_freed_reused_and_previous_map_edicts() {
        thread_local! {
            static EDICT: Cell<*mut edict_t> = const { Cell::new(ptr::null_mut()) };
        }
        unsafe extern "C" fn entity(index: i32) -> *mut edict_t {
            if index == 7 {
                EDICT.get()
            } else {
                ptr::null_mut()
            }
        }
        let mut edict = edict_t {
            serialnumber: 42,
            ..Default::default()
        };
        let edict = &raw mut edict;
        EDICT.set(edict);
        let engine = Engine {
            entity: Some(entity),
            ..Default::default()
        };
        activate_map();
        let handle = EntityHandle {
            index: 7,
            serial: 42,
            map: active_map().unwrap(),
        };
        assert!(handle.resolve(engine).is_some());
        // SAFETY: The fixture edict lives through the entire test.
        unsafe { (*edict).free = 1 };
        assert!(handle.resolve(engine).is_none());
        unsafe {
            (*edict).free = 0;
            (*edict).serialnumber = 43;
        }
        assert!(handle.resolve(engine).is_none());
        unsafe { (*edict).serialnumber = 42 };
        invalidate_map();
        assert!(handle.resolve(engine).is_none());
        activate_map();
        assert!(handle.resolve(engine).is_none());
        EDICT.set(ptr::null_mut());
    }

    #[test]
    fn tracing_rejects_nonfinite_coordinates() {
        assert_eq!(
            trace(
                [f32::NAN, 0.0, 0.0],
                [0.0; 3],
                TraceOptions::default(),
                None
            ),
            Err(TraceError::InvalidCoordinates)
        );
    }
}
