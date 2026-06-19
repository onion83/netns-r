#!/bin/bash
set -e

declare -A NS_IP=(
  [cmc]="192.168.11.41/24 fd83:11::41/64"
  [ctc]="192.168.11.42/24 fd83:11::42/64"
  [cuc]="192.168.11.43/24 fd83:11::43/64"
)

IPv4_Gateway="192.168.11.1"
IPv6_Gateway="fd83:11::1"

for ns in cmc cuc ctc; do
    echo "====== Build Netspace $ns ====== "

    (ip netns pids $ns | xargs kill -9) || true

    if ip netns list | awk '{print $1}' | grep -qx "$ns"; then
        echo "[SKIP] netns $ns already exists"
        ip netns exec $ns nohup /data/tools/swoole-cli /data/apps/dping/dping-server.php &
        continue
    fi

    read -r ns_ipv4 ns_ipv6 <<< "${NS_IP[$ns]}"
    ns_netdev="eth-$ns"

    ip netns add $ns
    ip link set dev $ns_netdev netns $ns

    ip netns exec $ns ip link set lo up
    ip netns exec $ns ip link set $ns_netdev up
    ip netns exec $ns ip addr add $ns_ipv4 dev $ns_netdev
    ip netns exec $ns ip -6 addr add $ns_ipv6 dev $ns_netdev
    ip netns exec $ns ip r add default via $IPv4_Gateway
    ip netns exec $ns ip -6 r add default via $IPv6_Gateway

    nohup ip netns exec $ns /data/tools/swoole-cli /data/apps/dping/dping-server.php &
done

declare -A NS_APPS=(
  [dping]="/data/tools/swoole-cli /data/apps/dping/dping-server.php"
)