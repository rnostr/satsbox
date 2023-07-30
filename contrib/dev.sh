BCLI='docker-compose exec --user bitcoind bitcoind  bitcoin-cli -regtest '
CLN_CLI='docker-compose exec cln lightning-cli --network=regtest '
LND_CLI='docker-compose exec lnd lncli --network=regtest '

txid="" 

DEBUG=0
C1='\033[0;32m' # green
C2='\033[0;33m' # orange
C3='\033[0;34m' # blue
C4='\033[0;31m' # red
NC='\033[0m'    # No Color

_die() {
    >&2 echo "$@"
    exit 1
}

_tit() {
    echo
    printf "${C1}==== %-20s ====${NC}\n" "$@"
}

_subtit() {
    printf "${C2} > %s${NC}\n" "$@"
}

_log() {
    printf "${C3}%s${NC}\n" "$@"
}

_trace() {
    [ "$DEBUG" != 0 ] && set -x
    "$@"
    { set +x; } 2>/dev/null
}

prepare_wallets() {
    for wallet in 'demo' 'miner'; do
        _subtit "creating wallet $wallet"
        _trace $BCLI createwallet $wallet >/dev/null
    done
}

init_blocks() {
  local count=$($BCLI getblockcount)
  if [ "$count" -gt 100 ];
    then
      _log "The block has been initialized"
    else
      gen_blocks 103
      _log "success"
  fi
}

gen_blocks() {
    local count="$1"
    _subtit "mining $count block(s)"
    if [ -z $($BCLI listwallets | grep 'miner') ];
      then
        _trace $BCLI loadwallet miner >/dev/null
    fi
    _trace $BCLI -rpcwallet=miner -generate $count >/dev/null
    sleep 1     # give electrs time to index
}

gen_addr() {
    local wallet="$1"
    _subtit "generating new address for wallet \"$wallet\""
    if [ -z $($BCLI listwallets | grep $wallet) ];
      then
        _trace $BCLI loadwallet $wallet >/dev/null
    fi
    addr=$(_trace $BCLI -rpcwallet=$wallet getnewaddress demo bech32m |tr -d '\r')
    _log $addr
}

fund() {
    local addr="$1"
    # send and mine
    _subtit "sending 2 btc to \"$addr\""
    txid="$(_trace $BCLI -rpcwallet=miner sendtoaddress ${addr} 2 |tr -d '\r')"
    gen_blocks 1
    _log "txid: $txid"
}

fund_cln_addr(){
  local addr=$(_trace $CLN_CLI -F newaddr bech32 | sed -n 's/^bech32=\(.*\)/\1/p')
  _log "cln addr: $addr"
  fund $addr
  # listfunds
}

fund_lnd_addr(){
  # p2wkh, p2tr
  local addr=$(_trace $LND_CLI newaddress p2wkh | grep address | cut -d '"' -f4)
  _log "lnd addr: $addr"
  fund $addr
  # listunspent
}

connect() {
  local clnid=$(_trace $CLN_CLI -F getinfo | grep id | cut -d= -f2-)
  local clnurl="$clnid@cln:9735"
  local lndid=$(_trace $LND_CLI getinfo | grep identity_pubkey | cut -d '"' -f4)
  local lndurl="$lndid@lnd:9735"
  _log "connect two node"
  _log "cln: $clnurl"
  _log "lnd: $lndurl"
  _trace $CLN_CLI connect $lndurl >/dev/null
}

# open two channel
open_to_lnd() {
  local lndid=$(_trace $LND_CLI getinfo | grep identity_pubkey | cut -d '"' -f4)
  _trace $CLN_CLI fundchannel "$lndid" 1000000
  gen_blocks 1
  gen_blocks 1
  gen_blocks 1
  gen_blocks 1
  gen_blocks 1
  gen_blocks 1
  sleep 2
}

open_to_cln() {
  local clnid=$(_trace $CLN_CLI -F getinfo | grep id | cut -d= -f2-)
  _trace $LND_CLI openchannel "$clnid" 1001000
  gen_blocks 1
  gen_blocks 1
  gen_blocks 1
  gen_blocks 1
  gen_blocks 1
  gen_blocks 1
  sleep 2
}

# pay invoice
pay() {
  local payment=$(_trace $LND_CLI addinvoice 1000 | grep payment_request | cut -d '"' -f4)
  # local payment=$(_trace $CLN_CLI invoice 1000 | grep bolt11 | cut -d '"' -f4)
  _log "payment: $payment"
  _trace $CLN_CLI pay "$payment" #> /dev/null
  # _trace $CLN_CLI listpays
  # _trace $LND_CLI lnd-cli sendpayment -f --pay_req "$payment" #> /dev/null
  # _trace $LND_CLI listpayments
}

# initialize for test
test() {
  _tit 'preparing bitcoin wallets'
  prepare_wallets
  _tit 'initial blocks'
  init_blocks
  sleep 1
  connect
  sleep 1
  fund_cln_addr
  fund_lnd_addr
  sleep 10
  open_to_cln
  sleep 10
  open_to_lnd
  sleep 20
  _trace $LND_CLI listchannels
}

# cmdline options
[ "$2" = "-v" ] && DEBUG=1

## asset issuance
#_tit 'issuing "USDT" asset'
#gen_utxo demo

case "$1" in
prepare)  
  # initial setup
  _tit 'preparing bitcoin wallets'
  prepare_wallets
  _tit 'initial blocks'
  init_blocks
  ;;

connect)  
  connect
  ;;

gen)  
  gen_blocks 1
  ;;

fund)  
  fund_cln_addr
  fund_lnd_addr
  ;;

open)  
  open_to_cln
  sleep 2
  open_to_lnd
  ;;

pay)  
  pay
  ;;

test)  
  test
  ;;


*)      
  echo "Usage: dev.sh {prepare|gen|connect|fund|open|pay|test}"
  exit 2
  ;;
esac
exit 0
