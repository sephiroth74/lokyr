# Lokyr's shell integration for bash, which the terminal brings in through
# ENV, this file, of a bash started in POSIX mode, which reads ENV alone:
# POSIX mode is turned off, with what it turned on, and the files that bash
# would have read are read, then the integration loads: the marks of OSC
# 133 around each prompt, its input, a command's output and its exit
# status, and the folder of OSC 7. The user's files are never touched. From
# bash 4.4, a command's output is marked from PS0, of no trap, so that the
# user's own DEBUG trap stays theirs; an older bash, as macOS's own, 3.2,
# which a user's file may load, marks it from a DEBUG trap, unless the
# user has one, which keeps `$_`.

if [ -n "${LOKYR_BASH_INJECT-}" ]; then
    builtin unset LOKYR_BASH_INJECT ENV
    if [ -n "${LOKYR_BASH_ENV+X}" ]; then
        builtin export ENV="$LOKYR_BASH_ENV"
        builtin unset LOKYR_BASH_ENV
    fi
    builtin set +o posix
    # POSIX mode turns `inherit_errexit` on, and leaving it does not turn it
    # off; bash before 4.4 has none.
    builtin shopt -u inherit_errexit 2>/dev/null
    if builtin shopt -q login_shell; then
        [ -r /etc/profile ] && builtin source /etc/profile
        for _lokyr_file in "$HOME/.bash_profile" "$HOME/.bash_login" "$HOME/.profile"; do
            if [ -r "$_lokyr_file" ]; then
                builtin source "$_lokyr_file"
                break
            fi
        done
    elif [ -r "$HOME/.bashrc" ]; then
        builtin source "$HOME/.bashrc"
    fi
    builtin unset _lokyr_file
fi

if [[ $- == *i* && -z "${_lokyr_integration-}" ]]; then
    _lokyr_integration=1
    # Whether a command ran since the last prompt, which PS0 sets as bash
    # shows it, once it has read a command and before it runs it: never for
    # an empty line, nor for a key's command of `bind -x`, as fzf's.
    _lokyr_running=0
    # Whether the prompt has been drawn, past which the next command is the
    # user's, for the DEBUG trap of a bash before 4.4.
    _lokyr_at_prompt=0
    # Whether this bash shows PS0, of 4.4 or later.
    _lokyr_ps0=0
    if ((BASH_VERSINFO[0] > 4 || (BASH_VERSINFO[0] == 4 && BASH_VERSINFO[1] >= 4))); then
        _lokyr_ps0=1
    fi

    # The folder, as a `file` URL of this machine, its bytes but letters,
    # digits and `/._~-` escaped.
    _lokyr_report_cwd() {
        builtin local LC_ALL=C url="" byte hex i
        for ((i = 0; i < ${#PWD}; i++)); do
            byte="${PWD:i:1}"
            case "$byte" in
                [A-Za-z0-9/._~-]) url+="$byte" ;;
                *)
                    builtin printf -v hex '%%%02X' "'$byte"
                    url+="$hex"
                    ;;
            esac
        done
        builtin printf '\e]7;file://%s%s\a' "$HOSTNAME" "$url"
    }

    # First of the prompt's commands: the last command's end, of its
    # status, and the folder; the status kept for the user's commands.
    _lokyr_prompt_start() {
        builtin local ret=$?
        _lokyr_at_prompt=0
        if [ "$_lokyr_running" = 1 ]; then
            builtin printf '\e]133;D;%s\a' "$ret"
            _lokyr_running=0
        fi
        _lokyr_report_cwd
        builtin return $ret
    }

    # Last of the prompt's commands: the prompt's marks in PS1, and the
    # output's at PS0's end, which the user's commands may have set anew.
    # The output's mark sets `_lokyr_running` as PS0 shows, of an
    # arithmetic expansion in a substring of no length, which shows
    # nothing; without `promptvars`, PS0 would show it as it is, and has
    # none.
    _lokyr_prompt_end() {
        builtin local ret=$?
        builtin local start='\[\e]133;A\a\]' input='\[\e]133;B\a\]'
        PS1="${PS1-}"
        PS1="${PS1//"$start"/}"
        PS1="${PS1//"$input"/}"
        PS1="${start}${PS1}${input}"
        if [ "$_lokyr_ps0" = 1 ]; then
            builtin local output='${_lokyr_running:0:$((_lokyr_running = 1, 0))}\e]133;C\a'
            # PS0, which bash leaves unset, of a default, as `set -u` takes
            # an unset variable's substitution for an error.
            builtin local ps0="${PS0-}"
            PS0="${ps0//"$output"/}"
            if builtin shopt -q promptvars; then
                PS0+="$output"
            fi
        fi
        _lokyr_at_prompt=1
        builtin return $ret
    }

    # Before each simple command, of a bash before 4.4: the first after the
    # prompt is the user's, whose output's mark comes, but a key's command
    # of `bind -x`, which runs at the prompt, of `READLINE_LINE`. It ends of
    # the `$_` that it was given, which the trap's command passes on, so
    # that `$_` stays the last argument of the user's command before.
    _lokyr_debug() {
        if [ -z "${READLINE_LINE+x}" ] && [ "$_lokyr_at_prompt" = 1 ] &&
            [[ "$BASH_COMMAND" != _lokyr_prompt_* ]]; then
            _lokyr_at_prompt=0
            _lokyr_running=1
            builtin printf '\e]133;C\a'
        fi
        : "$1"
    }

    # The prompt's commands around the user's, of lines apart, so that a
    # comment that ends the user's takes none of them; an array's, of bash
    # 5.1, its first and its last.
    if [[ "$(builtin declare -p PROMPT_COMMAND 2>/dev/null)" == "declare -a"* ]]; then
        PROMPT_COMMAND=(_lokyr_prompt_start "${PROMPT_COMMAND[@]}" _lokyr_prompt_end)
    else
        PROMPT_COMMAND=$'_lokyr_prompt_start\n'"${PROMPT_COMMAND-}"$'\n_lokyr_prompt_end'
    fi
    if [ "$_lokyr_ps0" = 0 ] && [ -z "$(builtin trap -p DEBUG)" ]; then
        builtin trap '_lokyr_debug "$_"' DEBUG
    fi
fi
