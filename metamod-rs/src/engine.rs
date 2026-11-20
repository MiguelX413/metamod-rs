use crate::{__ffi, Context, PlayerId, Server};
use metamod_sys::{
    EngineFunctions,
    raw::{
        MetamodAddServerCommand, MetamodCVarGetPointer, MetamodCVarGetString, MetamodCVarRegister,
        MetamodCVarSetString, MetamodCmd_Argc, MetamodCmd_Argv, MetamodGetPlayerAuthId,
        MetamodGetPlayerUserId, MetamodIndexOfEdict, MetamodPEntityOfEntIndex, MetamodServerPrint,
        MetamodSzFromIndex, MetamodTraceHull, MetamodTraceLine, cvar_t,
    },
};
use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    ffi::{CStr, CString, c_char},
    fmt, ptr,
};

macro_rules! engine_callbacks {
    ($($field:ident: $ty:ident = $native:ident),+ $(,)?) => {
        #[derive(Clone, Copy, Default)]
        pub(crate) struct Engine {
            $(pub(crate) $field: $ty,)+
        }

        impl Engine {
            /// The loader's table must have readable, initialized callback slots.
            pub(crate) unsafe fn read(table: *mut EngineFunctions) -> Self {
                // SAFETY: Copy just the supplied slots without borrowing foreign memory.
                unsafe { Self { $($field: ptr::addr_of!((*table).$native).read(),)+ } }
            }
        }
    };
}

engine_callbacks! {
    print: MetamodServerPrint = pfnServerPrint,
    index: MetamodIndexOfEdict = pfnIndexOfEdict,
    register_cvar: MetamodCVarRegister = pfnCVarRegister,
    find_cvar: MetamodCVarGetPointer = pfnCVarGetPointer,
    get_cvar: MetamodCVarGetString = pfnCVarGetString,
    set_cvar: MetamodCVarSetString = pfnCVarSetString,
    register_command: MetamodAddServerCommand = pfnAddServerCommand,
    argc: MetamodCmd_Argc = pfnCmd_Argc,
    argv: MetamodCmd_Argv = pfnCmd_Argv,
    entity: MetamodPEntityOfEntIndex = pfnPEntityOfEntIndex,
    userid: MetamodGetPlayerUserId = pfnGetPlayerUserId,
    auth_id: MetamodGetPlayerAuthId = pfnGetPlayerAuthId,
    trace_line: MetamodTraceLine = pfnTraceLine,
    trace_hull: MetamodTraceHull = pfnTraceHull,
    string_from_index: MetamodSzFromIndex = pfnSzFromIndex,
}

// Native registration is irreversible on the portable ABI. Bound the number
// of reservations as well as the storage per reservation.
const MAX_CONSOLE_REGISTRATIONS: usize = 256;
pub(crate) const MAX_ENGINE_STRING: usize = 4096;

pub(crate) type CommandCallback = unsafe extern "C" fn();

thread_local! {
    static CVARS: RefCell<HashSet<&'static CStr>> = RefCell::new(HashSet::new());
    static COMMANDS: RefCell<HashSet<&'static CStr>> = RefCell::new(HashSet::new());
    static READING_COMMAND: Cell<Option<bool>> = const { Cell::new(None) };
}

/// A command invocation with owned copies of the engine's argument tokens.
#[derive(Clone, Debug)]
pub struct Command {
    name: &'static CStr,
    args: Vec<CString>,
}

impl Command {
    /// The registered command name in lowercase.
    pub fn name(&self) -> &CStr {
        self.name
    }

    /// One zero-based argument after the command name.
    pub fn arg(&self, index: usize) -> Option<&CStr> {
        self.args.get(index).map(CString::as_c_str)
    }

    /// Parsed tokens after the command name, preserving non-UTF-8 bytes.
    pub fn args(&self) -> &[CString] {
        &self.args
    }
}

/// Owned engine information for a session validated during the lookup.
#[derive(Clone, Debug)]
pub struct PlayerInfo {
    pub id: PlayerId,
    pub user_id: Option<i32>,
    pub auth_id: Option<CString>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EngineError {
    Unavailable,
    RegistrationUnavailable,
    InvalidName,
    InvalidValue,
    AlreadyRegistered,
    UnknownCvar,
    RegistrationRejected,
    RegistrationLimit,
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "engine operation unavailable",
            Self::RegistrationUnavailable => "registration requires an attached plugin callback",
            Self::InvalidName => {
                "expected a name of 1–63 ASCII letters, digits, underscores or hyphens"
            }
            Self::InvalidValue => "expected a CVar value of at most 1023 bytes with no NUL",
            Self::AlreadyRegistered => "the console name is already registered",
            Self::UnknownCvar => "unknown CVar",
            Self::RegistrationRejected => "the engine did not register the CVar",
            Self::RegistrationLimit => "the plugin console registration limit was reached",
        })
    }
}

impl std::error::Error for EngineError {}

fn name(value: &str) -> Result<CString, EngineError> {
    if value.is_empty()
        || value.len() > 63
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(EngineError::InvalidName);
    }
    CString::new(value.to_ascii_lowercase()).map_err(|_| EngineError::InvalidName)
}

fn value(value: &str) -> Result<CString, EngineError> {
    if value.len() > 1023 {
        return Err(EngineError::InvalidValue);
    }
    CString::new(value).map_err(|_| EngineError::InvalidValue)
}

fn engine() -> Result<Engine, EngineError> {
    __ffi::engine().ok_or(EngineError::Unavailable)
}

fn available() -> Result<(), EngineError> {
    if __ffi::engine_available() {
        Ok(())
    } else {
        Err(EngineError::Unavailable)
    }
}

fn exists(engine: Engine, name: &CStr) -> Result<bool, EngineError> {
    let find = engine.find_cvar.ok_or(EngineError::Unavailable)?;
    available()?;
    // SAFETY: The synchronous lookup borrows a valid name. We never dereference the result.
    Ok(!unsafe { find(name.as_ptr()) }.is_null())
}

fn registration_available() -> Result<(), EngineError> {
    if __ffi::registration_available() {
        Ok(())
    } else {
        Err(EngineError::RegistrationUnavailable)
    }
}

fn reserve_name(name: CString, cvar: bool) -> Result<&'static CStr, EngineError> {
    CVARS.with(|cvars| {
        COMMANDS.with(|commands| {
            let mut cvars = cvars.borrow_mut();
            let mut commands = commands.borrow_mut();
            if commands.contains(name.as_c_str()) {
                return Err(EngineError::AlreadyRegistered);
            }
            if cvars.contains(name.as_c_str()) {
                return Err(if cvar {
                    EngineError::RegistrationRejected
                } else {
                    EngineError::AlreadyRegistered
                });
            }
            if cvars.len() + commands.len() >= MAX_CONSOLE_REGISTRATIONS {
                return Err(EngineError::RegistrationLimit);
            }
            let name: &'static CStr = Box::leak(name.into_boxed_c_str());
            if cvar {
                cvars.insert(name);
            } else {
                commands.insert(name);
            }
            Ok(name)
        })
    })
}

impl Server<'_, '_> {
    /// Register a server CVar from an attached callback. Use a plugin-specific name.
    pub fn register_cvar(&self, input: &str, default: &str) -> Result<(), EngineError> {
        registration_available()?;
        let name = name(input)?;
        let default = value(default)?;
        let engine = engine()?;
        let register = engine.register_cvar.ok_or(EngineError::Unavailable)?;
        if COMMANDS.with(|commands| commands.borrow().contains(name.as_c_str()))
            || exists(engine, &name)?
        {
            return Err(EngineError::AlreadyRegistered);
        }
        registration_available()?;
        // GoldSrc may retain the structure and name, and may replace the
        // string field with storage allocated by the engine. Never reclaim
        // these allocations or read the structure back as Rust-owned data.
        // Reserve before crossing FFI. Even a failed or interrupted
        // attempt may have handed the name and CVar storage to the host.
        let name = reserve_name(name, true)?;
        let initial = Box::leak(default.into_boxed_c_str());
        let cvar = Box::into_raw(Box::new(cvar_t {
            name: name.as_ptr(),
            string: initial.as_ptr(),
            ..Default::default()
        }));
        // SAFETY: The structure is writable and all input storage stays resident.
        unsafe { register(cvar) };
        if exists(engine, name)? {
            Ok(())
        } else {
            Err(EngineError::RegistrationRejected)
        }
    }

    /// Copy the current value. An empty value differs from an unregistered CVar.
    pub fn cvar(&self, input: &str) -> Result<Option<CString>, EngineError> {
        let name = name(input)?;
        let engine = engine()?;
        let get = engine.get_cvar.ok_or(EngineError::Unavailable)?;
        if !exists(engine, &name)? {
            return Ok(None);
        }
        available()?;
        // SAFETY: Copy the synchronous engine result before making another native call.
        let result = unsafe { get(name.as_ptr()) };
        available()?;
        Ok(unsafe { copy_string(result, MAX_ENGINE_STRING) })
    }

    /// Set an existing CVar through the engine, which copies the supplied value.
    pub fn set_cvar(&self, input: &str, new_value: &str) -> Result<(), EngineError> {
        let name = name(input)?;
        let value = value(new_value)?;
        let engine = engine()?;
        let set = engine.set_cvar.ok_or(EngineError::Unavailable)?;
        if !exists(engine, &name)? {
            return Err(EngineError::UnknownCvar);
        }
        available()?;
        // SAFETY: Both input strings stay live through the synchronous copy.
        unsafe { set(name.as_ptr(), value.as_ptr()) };
        Ok(())
    }

    /// Register a command for `Plugin::server_command` from an attached callback.
    /// GoldSrc cannot report native command-name collisions through this API.
    pub fn register_command(&self, input: &str) -> Result<(), EngineError> {
        registration_available()?;
        let callback = self.context.command;
        let name = name(input)?;
        let engine = engine()?;
        let register = engine.register_command.ok_or(EngineError::Unavailable)?;
        if engine.argc.is_none() || engine.argv.is_none() {
            return Err(EngineError::Unavailable);
        }
        if exists(engine, &name)? {
            return Err(EngineError::AlreadyRegistered);
        }
        registration_available()?;
        let registered = reserve_name(name, false)?;
        // SAFETY: The name and callback stay resident. No registry borrow
        // crosses the native call, which may invoke a command immediately.
        unsafe { register(registered.as_ptr(), Some(callback)) };
        Ok(())
    }
}

/// # Safety
/// A non-null pointer must identify a stable initialized byte range in one
/// allocation through its first NUL or `limit` bytes, whichever comes first.
/// This cannot check whether foreign memory is mapped or still live.
pub(crate) unsafe fn copy_string(value: *const c_char, limit: usize) -> Option<CString> {
    if value.is_null() {
        return None;
    }
    for offset in 0..limit {
        // SAFETY: The caller guarantees readability up to the terminator
        // or the limit. No subsequent byte is touched once a NUL is seen.
        if unsafe { value.add(offset).read() } == 0 {
            // SAFETY: The scan established this range and terminal NUL.
            let bytes = unsafe { std::slice::from_raw_parts(value.cast::<u8>(), offset + 1) };
            return CStr::from_bytes_with_nul(bytes).ok().map(CStr::to_owned);
        }
    }
    None
}

pub(crate) fn invalidate_command_read() {
    if READING_COMMAND.get().is_some() {
        READING_COMMAND.set(Some(true));
    }
}

/// # Safety
/// Called synchronously by the engine with a live command argument context
/// and on the attached server thread.
pub(crate) unsafe fn read_command() -> Option<Command> {
    if READING_COMMAND.get().is_some() {
        // GoldSrc holds arguments in a shared command buffer. A nested
        // invocation during acquisition makes the outer snapshot unreliable.
        READING_COMMAND.set(Some(true));
        return None;
    }

    struct Reading;
    impl Drop for Reading {
        fn drop(&mut self) {
            READING_COMMAND.set(None);
        }
    }
    READING_COMMAND.set(Some(false));
    let _reading = Reading;

    let engine = __ffi::engine()?;
    let argc = engine.argc?;
    let argv = engine.argv?;
    // SAFETY: The engine called the command trampoline with a live command context.
    let count = unsafe { argc() };
    if !(1..=128).contains(&count) {
        return None;
    }
    let mut remaining = 4096usize;
    let mut argument = |index| {
        if READING_COMMAND.get() != Some(false) || !__ffi::engine_available() {
            return None;
        }
        // SAFETY: Each index is within argc. Copy before the next native call.
        let value = unsafe { argv(index) };
        if value.is_null() || READING_COMMAND.get() != Some(false) || !__ffi::engine_available() {
            return None;
        }
        // SAFETY: The engine's token remains live until the next native call.
        let value = unsafe { copy_string(value, remaining) }?;
        remaining -= value.as_bytes_with_nul().len();
        Some(value)
    };
    let invoked = argument(0)?;
    let key = CString::new(invoked.to_bytes().to_ascii_lowercase()).ok()?;
    let name = COMMANDS.with(|commands| commands.borrow().get(key.as_c_str()).copied())?;
    let args = (1..count).map(argument).collect::<Option<_>>()?;
    Some(Command { name, args })
}

impl Context<'_> {
    /// Read engine information for exactly this tracked session. The result
    /// owns its strings, and no engine entity pointer escapes the lookup.
    pub fn player_info(&self, id: PlayerId) -> Option<PlayerInfo> {
        let engine = __ffi::engine()?;
        if !self.is_player_current(id) {
            return None;
        }
        // SAFETY: The current session's index originated in a native player hook.
        let handle = unsafe { crate::trace::tracked_entity(engine, id.slot().get()) }?;
        let lookup = || {
            if !self.is_player_current(id) {
                return None;
            }
            let edict = handle.resolve(engine)?;
            self.is_player_current(id).then_some(edict)
        };
        let user_id = if let Some(userid) = engine.userid {
            // SAFETY: The edict was just obtained and its session revalidated.
            let uid = unsafe { userid(lookup()?) };
            (uid > 0).then_some(uid)
        } else {
            None
        };
        let auth_id = if let Some(auth_id) = engine.auth_id {
            // SAFETY: Resolve and validate the edict again after any prior native call.
            let edict = lookup()?;
            let result = unsafe { auth_id(edict) };
            // SAFETY: A reentrant callback can change the occupant, but the
            // edict arena remains readable while the map epoch is unchanged.
            if !self.is_player_current(id) || !unsafe { handle.still_matches(edict) } {
                return None;
            }
            unsafe { copy_string(result, MAX_ENGINE_STRING) }
        } else {
            None
        };
        lookup()?;
        Some(PlayerInfo {
            id,
            user_id,
            auth_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_string_reads_stop_at_nul_or_budget() {
        // SAFETY: Each input contains a NUL or has at least the specified
        // number of initialized readable bytes.
        unsafe {
            assert_eq!(copy_string(c"".as_ptr(), 4096).as_deref(), Some(c""));
            assert_eq!(copy_string(c"abc".as_ptr(), 4).as_deref(), Some(c"abc"));
            let unterminated = *b"abc";
            assert!(copy_string(unterminated.as_ptr().cast(), 3).is_none());
            assert!(copy_string(ptr::null(), 3).is_none());
        }
    }

    #[test]
    fn irreversible_registrations_share_a_bounded_name_budget() {
        assert!(reserve_name(name("Test").unwrap(), true).is_ok());
        assert_eq!(
            reserve_name(name("test").unwrap(), true),
            Err(EngineError::RegistrationRejected)
        );
        assert_eq!(
            reserve_name(name("TEST").unwrap(), false),
            Err(EngineError::AlreadyRegistered)
        );
        for index in 1..MAX_CONSOLE_REGISTRATIONS {
            assert!(reserve_name(name(&format!("test{index}")).unwrap(), false).is_ok());
        }
        assert_eq!(
            reserve_name(name("one_more").unwrap(), true),
            Err(EngineError::RegistrationLimit)
        );
    }
}
