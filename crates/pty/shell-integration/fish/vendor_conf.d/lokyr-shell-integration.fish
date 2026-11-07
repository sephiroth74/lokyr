# Lokyr's shell integration for fish, which the terminal brings in through
# XDG_DATA_DIRS, whose folders' vendor_conf.d fish reads: the terminal's
# folder leaves XDG_DATA_DIRS first, then, in an interactive fish before
# 4, which tells them itself, the marks of OSC 133 around each prompt, its
# input, a command's output and its exit status, and the folder of OSC 7.
# The user's files are never touched.

if set -q LOKYR_SHELL_INTEGRATION_XDG_DIR
    if set -q XDG_DATA_DIRS
        set -l kept
        for dir in (string split : -- $XDG_DATA_DIRS)
            if test "$dir" != "$LOKYR_SHELL_INTEGRATION_XDG_DIR"
                set -a kept $dir
            end
        end
        if test (count $kept) -gt 0
            set -gx XDG_DATA_DIRS (string join : -- $kept)
        else
            set -e XDG_DATA_DIRS
        end
    end
    set -e LOKYR_SHELL_INTEGRATION_XDG_DIR
end

status is-interactive; or exit 0

# fish 4 and later tell the marks and the folder themselves.
if test (string split . -- $version)[1] -ge 4
    exit 0
end

# The folder, as a `file` URL of this machine.
function __lokyr_report_cwd
    printf '\e]7;file://%s%s\a' $hostname (string escape --style=url -- $PWD)
end

function __lokyr_prompt_start --on-event fish_prompt
    __lokyr_report_cwd
    printf '\e]133;A\a'
end

function __lokyr_preexec --on-event fish_preexec
    printf '\e]133;C\a'
end

function __lokyr_postexec --on-event fish_postexec
    printf '\e]133;D;%d\a' $status
end

# At the first prompt, once the user's config has loaded: the input's mark
# after the user's own prompt.
function __lokyr_wrap_prompt --on-event fish_prompt
    functions -e __lokyr_wrap_prompt
    if functions -q fish_prompt; and not functions -q __lokyr_user_prompt
        functions -c fish_prompt __lokyr_user_prompt
        function fish_prompt
            __lokyr_user_prompt
            printf '\e]133;B\a'
        end
    end
end
