#!/usr/bin/env zsh

# Lilas prompt theme for Oh My Zsh.
# Based on the bundled "refined" theme, with the colors of stylesheet.md at the
# root of the myconfig repository: lilac for the folder, mint for the branch,
# and the neon pink only for the prompt arrow and unsaved changes.

setopt prompt_subst

# A terminal without 24-bit color, such as the Linux console, gets the nearest
# of its own colors.
[[ "$COLORTERM" == (truecolor|24bit) ]] || zmodload zsh/nearcolor

autoload -Uz vcs_info

zstyle ':vcs_info:*' enable git hg bzr
zstyle ':vcs_info:*:*' check-for-changes true
zstyle ':vcs_info:*:*' unstagedstr '!'
zstyle ':vcs_info:*:*' stagedstr '+'
zstyle ':vcs_info:git:*' formats '%b' '%u%c'
zstyle ':vcs_info:git:*' actionformats '%b|%a' '%u%c'
zstyle ':vcs_info:hg:*' formats '%b' '%u%c'
zstyle ':vcs_info:bzr:*' formats '%b' '%u%c'

lilas_git_dirty() {
    command git rev-parse --is-inside-work-tree &>/dev/null || return
    if ! command git diff --quiet --ignore-submodules HEAD &>/dev/null; then
        print -n '%F{magenta}●%f'
    fi
}

lilas_repo_information() {
    local branch="${vcs_info_msg_0_}"
    local state="${vcs_info_msg_1_}"
    local out="%F{#b296cc}%~%f"

    if [[ -n "$branch" ]]; then
        out+=" %F{8}on %f%F{#9ee6c8}${branch}%f"
        [[ -n "$state" ]] && out+=" %F{red}${state}%f"
        local dirty="$(lilas_git_dirty)"
        [[ -n "$dirty" ]] && out+=" ${dirty}"
    fi

    print -n "$out"
}

lilas_cmd_exec_time() {
    local stop="${EPOCHSECONDS:-$(date +%s)}"
    local start="${cmd_timestamp:-$stop}"
    local elapsed=$((stop - start))

    ((elapsed > 5)) && print -n " %F{#ffb38a}${elapsed}s%f"
}

preexec() {
    cmd_timestamp="${EPOCHSECONDS:-$(date +%s)}"
}

precmd() {
    setopt localoptions nopromptsubst

    vcs_info
    print -P "\n$(lilas_repo_information)$(lilas_cmd_exec_time)"
    unset cmd_timestamp
}

lilas_ssh_info() {
    [[ -n "$SSH_TTY" || -n "$SSH_CONNECTION" || -n "$SSH_CLIENT" ]] || return
    print -n "%F{8}%n@%m%f"
}

PROMPT='%(?.%F{magenta}.%F{red})❯%f '
RPROMPT='$(lilas_ssh_info)'
