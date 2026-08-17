#!/usr/bin/env zsh

# Black & Pink prompt theme for Oh My Zsh.
# Based on the bundled "refined" theme, with colors mapped to the
# blacknpink terminal palette from the shared stylesheet.

setopt prompt_subst

autoload -Uz vcs_info

zstyle ':vcs_info:*' enable git hg bzr
zstyle ':vcs_info:*:*' check-for-changes true
zstyle ':vcs_info:*:*' unstagedstr '!'
zstyle ':vcs_info:*:*' stagedstr '+'
zstyle ':vcs_info:git:*' formats '%b' '%u%c'
zstyle ':vcs_info:git:*' actionformats '%b|%a' '%u%c'
zstyle ':vcs_info:hg:*' formats '%b' '%u%c'
zstyle ':vcs_info:bzr:*' formats '%b' '%u%c'

blacknpink_git_dirty() {
    command git rev-parse --is-inside-work-tree &>/dev/null || return
    if ! command git diff --quiet --ignore-submodules HEAD &>/dev/null; then
        print -n '%F{magenta}●%f'
    fi
}

blacknpink_repo_information() {
    local branch="${vcs_info_msg_0_}"
    local state="${vcs_info_msg_1_}"
    local out="%F{magenta}%~%f"

    if [[ -n "$branch" ]]; then
        out+=" %F{8}on %f%F{magenta}${branch}%f"
        [[ -n "$state" ]] && out+=" %F{red}${state}%f"
        local dirty="$(blacknpink_git_dirty)"
        [[ -n "$dirty" ]] && out+=" ${dirty}"
    fi

    print -n "$out"
}

blacknpink_cmd_exec_time() {
    local stop="${EPOCHSECONDS:-$(date +%s)}"
    local start="${cmd_timestamp:-$stop}"
    local elapsed=$((stop - start))

    ((elapsed > 5)) && print -n " %F{yellow}${elapsed}s%f"
}

preexec() {
    cmd_timestamp="${EPOCHSECONDS:-$(date +%s)}"
}

precmd() {
    setopt localoptions nopromptsubst

    vcs_info
    print -P "\n$(blacknpink_repo_information)$(blacknpink_cmd_exec_time)"
    unset cmd_timestamp
}

blacknpink_ssh_info() {
    [[ -n "$SSH_TTY" || -n "$SSH_CONNECTION" || -n "$SSH_CLIENT" ]] || return
    print -n "%F{8}%n@%m%f"
}

PROMPT='%(?.%F{magenta}.%F{red})❯%f '
RPROMPT='$(blacknpink_ssh_info)'
