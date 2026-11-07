# Lokyr's shell integration for bash, which the terminal brings in through
# ENV, this file, of a bash started in POSIX mode, which reads ENV alone:
# POSIX mode is turned off and the files that bash would have read are
# read, then the integration loads: the marks of OSC 133 around each
# prompt, its input, a command's output and its exit status, and the
# folder of OSC 7. The user's files are never touched. It works in bash
# 3.2, macOS's own, and later.

if [ -n "${LOKYR_BASH_INJECT-}" ]; then
    builtin unset LOKYR_BASH_INJECT ENV
    if [ -n "${LOKYR_BASH_ENV+X}" ]; then
        builtin export ENV="$LOKYR_BASH_ENV"
        builtin unset LOKYR_BASH_ENV
    fi
    builtin set +o posix
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
    # Whether a command ran since the last prompt, and whether the prompt
    # has been drawn, past which the next command is the user's.
    _lokyr_running=0
    _lokyr_at_prompt=0

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

    # Last of the prompt's commands: the prompt's marks in PS1, which the
    # user's commands may have set anew.
    _lokyr_prompt_end() {
        builtin local ret=$?
        builtin local start='\[\e]133;A\a\]' input='\[\e]133;B\a\]'
        PS1="${PS1//"$start"/}"
        PS1="${PS1//"$input"/}"
        PS1="${start}${PS1}${input}"
        _lokyr_at_prompt=1
        builtin return $ret
    }

    # Before each simple command: the first after the prompt is the
    # user's, whose output's mark comes.
    _lokyr_debug() {
        [ "$_lokyr_at_prompt" = 1 ] || builtin return 0
        case "$BASH_COMMAND" in
            _lokyr_prompt_*) builtin return 0 ;;
        esac
        _lokyr_at_prompt=0
        _lokyr_running=1
        builtin printf '\e]133;C\a'
    }

    PROMPT_COMMAND="_lokyr_prompt_start${PROMPT_COMMAND:+; ${PROMPT_COMMAND%;}}; _lokyr_prompt_end"
    # A trap of the user's own, as bash-preexec's, stays: the commands'
    # marks go without it.
    if [ -z "$(builtin trap -p DEBUG)" ]; then
        builtin trap '_lokyr_debug' DEBUG
    fi
fi
