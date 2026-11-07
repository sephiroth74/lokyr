# Lokyr's shell integration for zsh, which the terminal brings in through
# ZDOTDIR, this folder: the user's own ZDOTDIR is put back first, so that
# zsh reads the user's .zshenv, .zprofile, .zshrc and .zlogin as it would,
# then the integration loads, in an interactive shell. The user's files are
# never touched.

if [[ -n "${LOKYR_ZSH_ZDOTDIR+X}" ]]; then
    builtin export ZDOTDIR="$LOKYR_ZSH_ZDOTDIR"
    builtin unset LOKYR_ZSH_ZDOTDIR
else
    builtin unset ZDOTDIR
fi

{
    builtin typeset _lokyr_zshenv="${ZDOTDIR-$HOME}/.zshenv"
    [[ ! -r "$_lokyr_zshenv" ]] || builtin source -- "$_lokyr_zshenv"
} always {
    if [[ -o interactive ]]; then
        builtin typeset _lokyr_here="${${(%):-%x}:A:h}"
        builtin source -- "$_lokyr_here/lokyr-integration"
    fi
    builtin unset _lokyr_zshenv _lokyr_here
}
