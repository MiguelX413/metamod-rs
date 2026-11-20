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

pub(crate) type CommandCallback = unsafe extern "C" fn();

thread_local! {
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
        let name: &'static CStr = Box::leak(name.into_boxed_c_str());
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
        Ok(unsafe { copy_string(result) })
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
        let registered = COMMANDS
            .with(|commands| {
                let mut commands = commands.borrow_mut();
                if commands.contains(name.as_c_str()) {
                    return None;
                }
                let name: &'static CStr = Box::leak(name.into_boxed_c_str());
                commands.insert(name);
                Some(name)
            })
            .ok_or(EngineError::AlreadyRegistered)?;
        // SAFETY: The name and callback stay resident. No registry borrow
        // crosses the native call, which may invoke a command immediately.
        unsafe { register(registered.as_ptr(), Some(callback)) };
        Ok(())
    }
}

/// A non-null pointer must refer to a live NUL-terminated engine string.
pub(crate) unsafe fn copy_string(value: *const c_char) -> Option<CString> {
    if value.is_null() {
        return None;
    }
    // SAFETY: The caller ensures validity until the immediate copy finishes.
    Some(unsafe { CStr::from_ptr(value) }.to_owned())
}

pub(crate) fn read_command() -> Option<Command> {
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
        let value = unsafe { CStr::from_ptr(value) };
        remaining = remaining.checked_sub(value.to_bytes_with_nul().len())?;
        Some(value.to_owned())
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
        let entity = engine.entity?;
        let lookup = || {
            if !self.is_player_current(id) {
                return None;
            }
            // SAFETY: This slot belongs to a currently observed player.
            let edict = unsafe { entity(id.slot().get()) };
            (!edict.is_null() && self.is_player_current(id)).then_some(edict)
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
            let result = unsafe { auth_id(lookup()?) };
            if !self.is_player_current(id) {
                return None;
            }
            unsafe { copy_string(result) }
        } else {
            None
        };
        self.is_player_current(id).then_some(PlayerInfo {
            id,
            user_id,
            auth_id,
        })
    }
}
