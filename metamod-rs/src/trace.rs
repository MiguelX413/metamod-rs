use crate::{
    __ffi, Context, Server,
    engine::{Engine, copy_string},
};
use metamod_sys::{
    EngineGlobals,
    raw::{self, edict_t},
};
use std::{cell::Cell, ffi::CString, fmt, ptr};

thread_local! {
    // Keep the counter across deactivation so a new map cannot revive handles.
    static GLOBALS: Cell<*mut EngineGlobals> = const { Cell::new(ptr::null_mut()) };
    static LIMIT: Cell<i32> = const { Cell::new(0) };
    static MAP: Cell<(u64, bool)> = const { Cell::new((0, false)) };
}

/// # Safety
/// Non-null globals must remain readable on the server thread through shutdown.
pub(crate) unsafe fn set_globals(globals: *mut EngineGlobals) {
    GLOBALS.set(globals);
}

pub(crate) fn invalidate_map() {
    LIMIT.set(0);
    let (generation, _) = MAP.get();
    MAP.set((generation, false));
}

pub(crate) fn activate_map() {
    let (generation, _) = MAP.get();
    if let Some(generation) = generation.checked_add(1) {
        MAP.set((generation, true));
        let globals = GLOBALS.get();
        if !globals.is_null() {
            // SAFETY: The loader retains initialized globals and this is the
            // owner-thread post-activation hook for the current map.
            LIMIT.set(unsafe { ptr::addr_of!((*globals).maxEntities).read() });
        }
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

/// Broad engine classification derived from world index and edict flags.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntityKind {
    World,
    Client,
    FakeClient,
    Monster,
    Other,
}

/// Owned string keyvalues retained in GoldSrc's common entvars fields.
/// Zero string indices and null engine strings are represented by `None`.
/// GameDLL-specific map keys may never have been stored in these fields.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EntityKeyValues {
    pub classname: Option<CString>,
    pub globalname: Option<CString>,
    pub target: Option<CString>,
    pub targetname: Option<CString>,
    pub model: Option<CString>,
    pub netname: Option<CString>,
    pub message: Option<CString>,
}

/// An owned classification and keyvalue snapshot for an entity allocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntityInfo {
    pub entity: EntityHandle,
    pub kind: EntityKind,
    pub keyvalues: EntityKeyValues,
}

/// Owned spatial fields as reported by the engine at lookup time.
/// The snapshot remains readable after its entity handle expires.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntitySpatial {
    pub entity: EntityHandle,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub velocity: [f32; 3],
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntityError {
    Unavailable,
    MapChanged,
}

impl fmt::Display for EntityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "entity discovery requires an active map and edict table",
            Self::MapChanged => "the map changed during entity discovery",
        })
    }
}

impl std::error::Error for EntityError {}

fn vector(v: raw::Vector) -> [f32; 3] {
    [v.x, v.y, v.z]
}

fn find_entity(
    lookup: impl FnOnce(i32) -> *mut edict_t,
    index: i32,
    map: u64,
) -> Result<Option<EntityHandle>, EntityError> {
    if active_map() != Some(map) {
        return Err(EntityError::MapChanged);
    }
    let edict = lookup(index);
    if active_map() != Some(map) {
        return Err(EntityError::MapChanged);
    }
    // SAFETY: The lookup supplied a current-map edict or null and no
    // other native call intervened.
    Ok(unsafe { live_serial(edict) }.map(|serial| EntityHandle { index, serial, map }))
}

impl Context<'_> {
    /// Discover a live engine-visible edict by index, including world index zero.
    /// Out-of-range and vacant indices return `Ok(None)`.
    pub fn entity_at(&self, index: i32) -> Result<Option<EntityHandle>, EntityError> {
        let (map, limit, lookup) = discovery()?;
        if !(0..limit).contains(&index) {
            return Ok(None);
        }
        // SAFETY: The index lies within the active engine edict table.
        find_entity(|index| unsafe { lookup(index) }, index, map)
    }

    /// Scan the engine-visible edict table in index order. Each handle
    /// describes the allocation seen at its lookup and can expire later.
    /// A map transition interrupts the scan with `EntityError::MapChanged`.
    pub fn entities(&self) -> Result<Vec<EntityHandle>, EntityError> {
        let (map, limit, lookup) = discovery()?;
        let mut found = Vec::new();
        for index in 0..limit {
            // SAFETY: The index lies within the active engine edict table.
            if let Some(entity) = find_entity(|index| unsafe { lookup(index) }, index, map)? {
                found.push(entity);
            }
        }
        Ok(found)
    }

    /// Copy engine classification and common string keyvalues. Returns
    /// `None` if lookup is unavailable or the allocation expires mid-read.
    pub fn entity_info(&self, entity: EntityHandle) -> Option<EntityInfo> {
        let engine = __ffi::engine()?;
        let lookup = engine.string_from_index?;
        let edict = entity.resolve(engine)?;

        // SAFETY: The edict has just been validated. Copy flags and offsets
        // before invoking any engine function that could invalidate it.
        let (flags, indices) = unsafe {
            let vars = ptr::addr_of!((*edict).v);
            (
                ptr::addr_of!((*vars).flags).read(),
                [
                    ptr::addr_of!((*vars).classname).read(),
                    ptr::addr_of!((*vars).globalname).read(),
                    ptr::addr_of!((*vars).target).read(),
                    ptr::addr_of!((*vars).targetname).read(),
                    ptr::addr_of!((*vars).model).read(),
                    ptr::addr_of!((*vars).netname).read(),
                    ptr::addr_of!((*vars).message).read(),
                ],
            )
        };
        let kind = if entity.index() == 0 {
            EntityKind::World
        } else if flags & raw::FL_FAKECLIENT as i32 != 0 {
            EntityKind::FakeClient
        } else if flags & raw::FL_CLIENT as i32 != 0 {
            EntityKind::Client
        } else if flags & raw::FL_MONSTER as i32 != 0 {
            EntityKind::Monster
        } else {
            EntityKind::Other
        };
        let mut strings = [const { None }; 7];
        for (index, value) in indices.into_iter().zip(&mut strings) {
            if index == 0 {
                continue;
            }
            if active_map() != Some(entity.map) {
                return None;
            }
            // SAFETY: The bit pattern comes from the engine's own string_t.
            // GoldSrc uses signed offsets at this callback boundary.
            let string = unsafe { lookup(index as i32) };
            if active_map() != Some(entity.map) {
                return None;
            }
            // SAFETY: The current-map engine string can be read until the
            // next native call. Copy it before revalidating the edict.
            *value = unsafe { copy_string(string) };
            entity.resolve(engine)?;
        }
        let [
            classname,
            globalname,
            target,
            targetname,
            model,
            netname,
            message,
        ] = strings;
        Some(EntityInfo {
            entity,
            kind,
            keyvalues: EntityKeyValues {
                classname,
                globalname,
                target,
                targetname,
                model,
                netname,
                message,
            },
        })
    }

    /// Copy current spatial fields after validating the allocation generation.
    /// Bounds are the engine's world-space absolute bounds. A stale or
    /// unresolved handle returns `None`.
    pub fn entity_spatial(&self, entity: EntityHandle) -> Option<EntitySpatial> {
        let edict = entity.resolve(__ffi::engine()?)?;
        // SAFETY: The handle resolved to a live edict. Copy initialized
        // vector fields without creating references or calling the engine.
        unsafe {
            let vars = ptr::addr_of!((*edict).v);
            Some(EntitySpatial {
                entity,
                origin: vector(ptr::addr_of!((*vars).origin).read()),
                angles: vector(ptr::addr_of!((*vars).angles).read()),
                velocity: vector(ptr::addr_of!((*vars).velocity).read()),
                bounds_min: vector(ptr::addr_of!((*vars).absmin).read()),
                bounds_max: vector(ptr::addr_of!((*vars).absmax).read()),
            })
        }
    }
}

type EdictLookup = unsafe extern "C" fn(i32) -> *mut edict_t;

fn discovery() -> Result<(u64, i32, EdictLookup), EntityError> {
    let map = active_map().ok_or(EntityError::Unavailable)?;
    let limit = LIMIT.get();
    let lookup = __ffi::engine()
        .and_then(|engine| engine.entity)
        .ok_or(EntityError::Unavailable)?;
    if limit <= 0 {
        return Err(EntityError::Unavailable);
    }
    Ok((map, limit, lookup))
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
