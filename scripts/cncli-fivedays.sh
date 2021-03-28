#!/usr/bin/env bash

CARDANO_START=$(date +%s -d "2017-09-23")
CARDANO_START_DAY=$(( CARDANO_START / 86400 ))
NOW_TIMESTAMP=$(date +%s)
NOW_DAY=$(( NOW_TIMESTAMP / 86400 ))
DAYS_SINCE_CARDANO_START=$(( NOW_DAY - CARDANO_START_DAY ))
RESULT=$(( DAYS_SINCE_CARDANO_START % 5 ))

if [[ "$RESULT" == "0" ]]; then
    echo "Exit Success"
    exit 0
else
    echo "Exit Failure"
    exit 1
fi
