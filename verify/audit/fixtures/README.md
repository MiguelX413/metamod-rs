# Runtime identity transcript inputs

- `hlds-captured.txt` contains three consecutive console lines from the Linux i686 HLDS log attached to [ValveSoftware/halflife issue #1653](https://github.com/ValveSoftware/halflife/issues/1653). This is a third-party capture, not a local run.
- `classic-synthetic.txt` uses the version, GameDLL and plugin-list formats from the [pinned classic Metamod commands](https://github.com/alliedmodders/metamod-hl1/blob/18a10db686702e8ae9e0fcb5b5febf8881dc9c2d/metamod/commands_meta.cpp) and [fixed plugin-list columns](https://github.com/alliedmodders/metamod-hl1/blob/18a10db686702e8ae9e0fcb5b5febf8881dc9c2d/metamod/mlist.cpp#L745-L780). The plugin row and staged path are synthetic.
- `rehlds-metamod-r-synthetic.txt` uses the [ReHLDS version command](https://github.com/rehlds/ReHLDS/blob/master/rehlds/engine/host_cmd.cpp#L2443-L2455), [Metamod-R commands](https://github.com/rehlds/Metamod-R/blob/master/metamod/src/commands_meta.cpp), and [dynamic plugin-list columns](https://github.com/rehlds/Metamod-R/blob/master/metamod/src/mlist.cpp#L575-L629). The combined output, test versions, log prefix, and Windows extended path are synthetic.

Tests supply synthetic mapped-module evidence separately. These files exercise parser behavior and establish no compatibility result for the represented configurations.
