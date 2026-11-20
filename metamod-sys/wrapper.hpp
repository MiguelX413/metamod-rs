#include <extdll.h>
#include <meta_api.h>
#include <h_export.h>

// Give bindgen a name for the utilities' anonymous callback type.
using MetamodLog = decltype(mutil_funcs_t::pfnLogConsole);
using MetamodServerPrint = decltype(enginefuncs_t::pfnServerPrint);
using MetamodIndexOfEdict = decltype(enginefuncs_t::pfnIndexOfEdict);
