#!/bin/zsh
clear
printf '\e[1;38;2;255;180;60m  Termoil VT inside Unluminous\e[0m\n\n'
# 24-bit gradient
for i in {0..71}; do r=$(( 255 - i*255/71 )); g=$(( i*510/71 > 255 ? 510 - i*510/71 : i*510/71 )); b=$(( i*255/71 )); printf "\e[48;2;${r};${g};${b}m "; done; printf '\e[0m\n'
# half-block gradient (two colours per cell)
for i in {0..71}; do printf "\e[38;2;$((i*3));80;$((255-i*3))m\e[48;2;255;$((i*3));80m▀"; done; printf '\e[0m\n\n'
# styles
printf '\e[1mbold\e[0m  \e[3mitalic\e[0m  \e[2mfaint\e[0m  \e[9mstrike\e[0m  \e[53moverline\e[0m  \e[7mreverse\e[0m\n'
# five underline styles, coloured
printf '\e[4:1;58;2;255;80;80msingle\e[0m  \e[4:2;58;2;80;200;255mdouble\e[0m  \e[4:3;58;2;255;60;60mcurly\e[0m  \e[4:4;58;2;120;255;120mdotted\e[0m  \e[4:5;58;2;255;200;0mdashed\e[0m\n\n'
# 256 palette
for i in {16..231}; do printf "\e[48;5;${i}m \e[0m"; (( (i-15) % 72 == 0 )) && printf '\n'; done; printf '\n'
# unicode
printf 'emoji: 👨‍👩‍👧‍👦 🇯🇵 🏳️‍🌈 🦀   wide: 日本語 한국어 中文   combining: é ñ̃ a͂\n'
printf 'box: ┌──┬──┐ ╭──╮ ╔══╗  blocks: ▁▂▃▄▅▆▇█  braille: ⣿⡷⠿⢾⣽\n'
printf '    ├──┼──┤ ╰──╯ ╚══╝  shades: ░▒▓█\n\n'
# hyperlink (OSC 8)
printf 'link: \e]8;;https://github.com/Black-Rainbow-Labs/Termoil\e\\Termoil on GitHub\e]8;;\e\\\n'
