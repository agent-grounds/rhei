# Tab Completions

Rhei can generate shell completion scripts for Bash, Zsh, Fish, PowerShell,
and Elvish:

```bash
rhei completions <shell>
```

The generated completions are dynamic. Shells call back into the installed
`rhei` binary, so completions stay aligned with the current command tree and
can offer the nearest copy of each template name for `rhei instantiate <TAB>`.
Discovery checks `.agent-grounds/rhei/templates/` and then deprecated
`.agents/rhei/templates/` at every ancestor before user templates and the
built-in library (§FS-rhei-templates.1.2).

## Install for the Current User

After installing the `rhei` binary, run the command for your shell:

```bash
rhei completions bash --install
rhei completions zsh --install
rhei completions fish --install
rhei completions powershell --install
rhei completions elvish --install
```

Run only the line that matches your shell unless you intentionally want to
install completions for multiple shells.

Use `--dry-run` to print the destination without writing files:

```bash
rhei completions fish --install --dry-run
```

## Default User Paths

| Shell | Command | Installed file |
|-------|---------|----------------|
| Bash | `rhei completions bash --install` | `${XDG_DATA_HOME:-~/.local/share}/bash-completion/completions/rhei` |
| Zsh | `rhei completions zsh --install` | `~/.zfunc/_rhei` |
| Fish | `rhei completions fish --install` | `${XDG_CONFIG_HOME:-~/.config}/fish/completions/rhei.fish` |
| PowerShell | `rhei completions powershell --install` | `${XDG_CONFIG_HOME:-~/.config}/powershell/rhei-completions.ps1` |
| Elvish | `rhei completions elvish --install` | `${XDG_CONFIG_HOME:-~/.config}/elvish/lib/rhei-completions.elv` |

`rhei completions <shell> --install` writes the completion file, but it does
not edit shell startup files such as `.bashrc`, `.zshrc`, `config.fish`,
PowerShell profiles, or `rc.elv`.

## Shell Setup Notes

### Bash

Bash completion requires the standard `bash-completion` integration to be
available in your shell. After installing:

```bash
rhei completions bash --install
```

Open a new shell, or source your bash completion setup again if your
distribution requires it.

### Zsh

Rhei installs Zsh completions to `~/.zfunc/_rhei`. Make sure `~/.zfunc` is in
`fpath` before `compinit` runs:

```zsh
fpath=("$HOME/.zfunc" $fpath)
autoload -Uz compinit
compinit
```

Add those lines to `.zshrc` if your shell does not already load `~/.zfunc`.
Then install the completion file:

```bash
rhei completions zsh --install
```

Open a new shell or run `exec zsh`.

### Fish

Fish automatically loads completions from its completions directory:

```bash
rhei completions fish --install
```

Open a new shell, or source the installed file:

```fish
source ~/.config/fish/completions/rhei.fish
```

### PowerShell

Install the generated completer:

```powershell
rhei completions powershell --install
```

If PowerShell does not load it automatically, source the generated file from
your profile:

```powershell
. ~/.config/powershell/rhei-completions.ps1
```

Use this command to find your active profile path:

```powershell
$PROFILE
```

### Elvish

Install the generated completer:

```bash
rhei completions elvish --install
```

If Elvish does not load it automatically, source the generated file from
`rc.elv`:

```elvish
use ~/.config/elvish/lib/rhei-completions.elv
```

## Install System-Wide

Package scripts and local install scripts can install the binary first, then
install completion files into system directories:

```bash
rhei completions bash --install --system
rhei completions zsh --install --system
rhei completions fish --install --system
rhei completions powershell --install --system
rhei completions elvish --install --system
```

System-wide install paths are:

| Shell | Installed file |
|-------|----------------|
| Bash | `/usr/local/share/bash-completion/completions/rhei` |
| Zsh | `/usr/local/share/zsh/site-functions/_rhei` |
| Fish | `/usr/local/share/fish/vendor_completions.d/rhei.fish` |
| PowerShell | `/usr/local/share/powershell/Completions/rhei-completions.ps1` |
| Elvish | `/usr/local/share/elvish/lib/rhei-completions.elv` |

System-wide installs may require elevated permissions depending on the target
directory.

## Generate Without Installing

To preview or manually place a completion script, omit `--install`:

```bash
rhei completions bash
```

To write to an explicit path:

```bash
rhei completions zsh --output ~/.zfunc/_rhei
```
