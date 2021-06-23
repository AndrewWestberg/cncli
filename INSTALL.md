# Install & Automate CNCLI

## Installation

You can install CNCLI using either the release binaries or compiling the Rust code. Both installation examples hereby illustrated are instructions for an Ubuntu Linux server and use standard system paths from the Linux [File System Hierarchy Standard](https://en.wikipedia.org/wiki/Filesystem_Hierarchy_Standard).

### Install the binary release

To install CNCLI from a binary release, download the [latest release](https://github.com/AndrewWestberg/cncli/releases) and extract it in the ```/usr/local/bin/``` directory of the ```block producing node``` server of your stake pool. Adjust the ```<latest_release_version>``` variable in the command to the latest release available:

```bash
curl -sLJ https://github.com/AndrewWestberg/cncli/releases/download/v<latest_release_version>/cncli-<latest_release_version>-x86_64-unknown-linux-gnu.tar.gz -o /tmp/cncli-<latest_release_version>-x86_64-unknown-linux-gnu.tar.gz
```

```bash
tar xzvf /tmp/cncli-<latest_release_version>-x86_64-unknown-linux-gnu.tar.gz -C /usr/local/bin/
```

### Compile from source

#### Prepare RUST environment

```bash
mkdir -p $HOME/.cargo/bin
```

```bash
chown -R $USER\: $HOME/.cargo
```

```bash
touch $HOME/.profile
```

```bash
chown $USER\: $HOME/.profile
```

#### Install rustup - proceed with default install (option 1)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

```bash
source $HOME/.cargo/env
```

```bash
rustup install stable
```

```bash
rustup default stable
```

```bash
rustup update
```

```bash
rustup component add clippy rustfmt
```

#### Install dependencies and build cncli

Adjust the ```<latest_tag_name>``` variable in the command to the latest tag available:

```bash
source $HOME/.cargo/env
```

```bash
sudo apt-get update -y && sudo apt-get install -y automake build-essential pkg-config libffi-dev libgmp-dev libssl-dev libtinfo-dev libsystemd-dev zlib1g-dev make g++ tmux git jq wget libncursesw5 libtool autoconf
```

```bash
git clone --recurse-submodules https://github.com/AndrewWestberg/cncli
```

```bash
cd cncli
```

```bash
git checkout <latest_tag_name>
```

```bash
cargo install --path . --force
```

```bash
cncli --version
```

### Checking that cncli is properly installed

Run the following command to check if cncli is correctly installed and available in your system ```PATH``` variable:

```bash
command -v cncli
```

It should return ```/usr/local/bin/cncli```.

### Updating cncli from earlier versions

Adjust the ```<latest_tag_name>``` variable in the command to the latest tag available:

```bash
rustup update
```

```bash
cd cncli
```

```bash
git fetch --all --prune
```

```bash
git checkout <latest_tag_name>
```

```bash
cargo install --path . --force
```

```bash
cncli --version
```

## Cross Platform build with Nix + Flakes

We are going to to build cncli with [Nix](https://nixos.org/guides/install-nix.html) and [Nix Flakes](https://www.tweag.io/blog/2020-05-25-flakes/)

### Install Nix + Flakes

```bash
# Nix single user install
sh <(curl -L https://nixos.org/nix/install)
source ~/.nix-profile/etc/profile.d/nix.sh

# Configure Nix to also use the binary cache from IOHK
# Enable the experimental flakes feature
mkdir -p ~/.config/nix
cat << EOF > ~/.config/nix/nix.conf
trusted-public-keys = hydra.iohk.io:f/Ea+s+dFdN+3Y/G+FDgSq+a5NEWhJGzdjvKNGv0/EQ= cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY=
substituters = https://hydra.iohk.io https://cache.nixos.org
experimental-features = nix-command flakes
EOF

# Install Flakes
nix-shell -I nixpkgs=channel:nixos-20.03 --packages nixFlakes

# Test Nix+Flakes
nix flake show github:AndrewWestberg/cncli
github:AndrewWestberg/cncli/f8ea45b5e01bed81fbb3b848916219838786cd10
├───devShell
│   ├───aarch64-linux: development environment 'nix-shell'
│   └───x86_64-linux: development environment 'nix-shell'
├───overlay: Nixpkgs overlay
└───packages
    ├───aarch64-linux
    │   └───cncli: package 'cncli-3.1.0'
    └───x86_64-linux
        └───cncli: package 'cncli-3.1.0'
```

### Build the binary

We can now build cncli in a nix-shell that has flakes enabled

```bash
$ nix-shell -I nixpkgs=channel:nixos-20.03 --packages nixFlakes

[nix-shell:~/git/cncli]$ nix build .#cncli
```

### Build Troubleshooting

The Nix Flake build process requires plenty of file resources in $TEMPDIR.
In case you run into ...

* No space left on device
* Too many open files

Have a look [over here](https://github.com/AndrewWestberg/cncli/issues/83#issuecomment-868287041) on how to possibly fix this.

## Automation

This automation section of the guide assumes:

1. that you have installed ```cardano-node``` and ```cardano-cli``` in the standard path ```/usr/local/bin/```
2. that you have installed ```cncli``` in the standard path ```/usr/local/bin/```
3. that your block producing node port is ```3000```
4. that you sync the ```cncli.db``` in ```/root/scripts/```
5. that you dump the ```ledger-state.json``` in ```/root/scripts/```
6. that you have placed and are running the helper scripts as ```root``` from ```/root/scripts/```
7. that you setup the ```cronjobs``` in the ```crontab``` of user ```root```
8. that you have placed your pool ```pooltool.json``` file in ```/root/scripts/```
9. that your ```cardano-node``` user home is ```/home/cardano-node/```
10. that your ```/home/cardano-node/``` directory contains all ```cardano-node``` directories (```config```, ```db```, ```keys``` and ```socket```)
11. that your socket is ```/home/cardano-node/socket/node.socket```

**Note**: should you need to adjust paths, please do so after downloading the scripts and before configuring the services.

### Dependencies

The helper scripts rely on ```jq```, please install it with:

```bash
sudo apt-get install -y jq
```

### PoolTool Integration

CNCLI can send your tip and block slots to [PoolTool](https://pooltool.io/). To do this, it requires that you set up a ```pooltool.json``` file containing your PoolTool API key and stake pool details. Your PoolTool API key can be found on your pooltool profile page. Here's an example ```pooltool.json``` file. Please update with your pool information:

```json
{
    "api_key": "XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX",
    "pools": [
        {
            "name": "BCSH2",
            "pool_id": "00beef284975ef87856c1343f6bf50172253177fdebc756524d43fc1",
            "host" : "127.0.0.1",
            "port": 3000
        }
    ]
}
```

### Systemd Services

CNCLI ```sync``` and ```sendtip``` can be easily enabled as ```systemd``` services. When enabled as ```systemd``` services:

- ```sync``` will continuously keep the ```cncli.db``` database synchronized.
- ```sendtip``` will continuously send your stake pool ```tip``` to PoolTool.

To set up ```systemd```:

- Copy the following to ```/etc/systemd/system/cncli-sync.service```

```text
[Unit]
Description=CNCLI Sync
After=multi-user.target

[Service]
Type=simple
Restart=always
RestartSec=5
LimitNOFILE=131072
ExecStart=/usr/local/bin/cncli sync --host 127.0.0.1 --port 3000 --db /root/scripts/cncli.db
KillSignal=SIGINT
SuccessExitStatus=143
StandardOutput=syslog
StandardError=syslog
SyslogIdentifier=cncli-sync

[Install]
WantedBy=multi-user.target
```

- Copy the following to ```/etc/systemd/system/cncli-sendtip.service```

```text
[Unit]
Description=CNCLI Sendtip
After=multi-user.target

[Service]
Type=simple
Restart=always
RestartSec=5
LimitNOFILE=131072
ExecStart=/usr/local/bin/cncli sendtip --cardano-node /usr/local/bin/cardano-node --config /root/scripts/pooltool.json
KillSignal=SIGINT
SuccessExitStatus=143
StandardOutput=syslog
StandardError=syslog
SyslogIdentifier=cncli-sendtip

[Install]
WantedBy=multi-user.target
```

- To enable and run the above services, run:

```bash
sudo systemctl daemon-reload
```

```bash
sudo systemctl start cncli-sync.service
```

```bash
sudo systemctl start cncli-sendtip.service
```

### Helper Scripts

Besides setting up the ```systemd``` services, there are a couple of more automation that CNCLI can help you with. We have devised a few scripts that will be invoked daily with ```crontab``` and that will take care of:

1. calculating the ```next``` epoch assigned slots (with ```cncli leaderlog```)
2. send the ```previous``` and ```current``` assigned slots to PoolTool (with ```cncli sendslots```).
3. optionally: query the ```ledger-state``` and save it to a ```ledger-state.json``` file.

Although, by default, the ```cncli-leaderlog.sh``` script will calculate the ```next``` epoch ```leaderlog```, it can also be run manually to also calculate the ```previous``` and ```current``` epoch slots (adjust the time zone to better suit your location):

```bash
bash /root/scripts/cncli-leaderlog.sh previous UTC
```

```bash
bash /root/scripts/cncli-leaderlog.sh current UTC
```

```bash
bash /root/scripts/cncli-leaderlog.sh next UTC
```

#### Download the scripts

You can get the scripts from [here](scripts). Place them under ```/root/scripts/``` of the block producing node server of your pool. If you don't have that directory, create it by running the following command as ```root```:

```bash
mkdir /root/scripts/
```

**Important**: at the very least, remember to change the pool id in the ```cncli-leaderlog.sh``` script to match your pool.

#### Crontab

To set up the ```cronjobs```, run ```crontab -e``` as ```root``` and paste the following into it and save.

Please note it will set timezone for your user's crontab to UTC. If you have other cronjobs running that require a different timezone, you should place a new script in `/etc/cron.d` with these these cronjobs.

```text
# set timezone to UTC for these cronjobs to correctly time epoch start
CRON_TZ="UTC"

# calculate slots assignment for the next epoch
15 21 * * * /root/scripts/cncli-fivedays.sh && /root/scripts/cncli-leaderlog.sh
# send previous and current epochs slots to pooltool
15 22 * * * /root/scripts/cncli-fivedays.sh && /root/scripts/cncli-sendslots.sh
```

Optionally set up a cronjob to dump the ledger-state, every day at 3:15 PM.

```text
# query ledger-state and dump to /root/scripts/ledger-state.json
15 15 * * * /root/scripts/ledger-dump.sh
```
