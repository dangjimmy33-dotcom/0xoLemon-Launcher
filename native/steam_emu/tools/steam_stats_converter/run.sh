#!/usr/bin/env bash
# Thin wrapper — forwards all arguments to steam_stats_converter.py
# Examples:
#   ./run.sh --appid 1234 --st2gse
#   ./run.sh --appid 1234 --gse2st
#   ./run.sh --appid 1234 --info
#   ./run.sh --help
python3 "$(dirname "$0")/steam_stats_converter.py" "$@"
