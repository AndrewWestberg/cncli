#!/usr/bin/env bash

function getStatus() {
    local result
    result=$(/usr/local/bin/cncli status \
        --db /root/scripts/cncli.db \
        --byron-genesis /home/cardano-node/config/mainnet-byron-genesis.json \
        --shelley-genesis /home/cardano-node/config/mainnet-shelley-genesis.json \
        | jq -r .status
    )
    echo "$result"
}

function sendSlots() {
    /usr/local/bin/cncli sendslots \
        --db /root/scripts/cncli.db \
        --byron-genesis /home/cardano-node/config/mainnet-byron-genesis.json \
        --shelley-genesis /home/cardano-node/config/mainnet-shelley-genesis.json \
        --config /root/scripts/pooltool.json
}

statusRet=$(getStatus)

if [[ "$statusRet" == "ok" ]]; then
    mv /root/scripts/sendslots.log /root/scripts/sendslots."$(date +%F-%H%M%S)".log
    sendSlots > /root/scripts/sendslots.log
    find . -name "sendslots.*.log" -mtime +15 -exec rm -f '{}' \;
else
    echo "CNCLI database not synced!!!"
fi

exit 0
