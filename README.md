# UE Cell Tracker

## Functionalities

* Read cell information from [Milesight router /cgi interface](https://support.milesight-iot.com/support/solutions/articles/73000514140-how-to-use-milesight-router-http-api-) or [DevicePublisher](https://github.com/bastian-src/DevicePublisher)
* Translate cell information to cell DL/UL frequency
* Write an NG-Scope config accordingly and start it
* Retrieve UE RNTI using UL RNTI matching
* Transmit UE cell allocation information to target address
* Log cell metrics and UE allocation information

## Setup

### Determine the DevicePublisher IP

In case you are using the DevicePublsher app and connected to your device via
USB-Tethering, the following command might come in handy to determine your
smartphone IP. It shows the dns routes per interface:

```
sudo resolvectl dns
```

In case your device gets only an IPv6 address, you can request an IPv6 by
configuring the network interface further. Therefore, add the following
udev rule to apply a static naming schema to your USB interface:

``` /etc/udev/rules.d/99-usb-tethering.rules
SUBSYSTEM=="net", ACTION=="add", DRIVERS=="rndis_host", ATTR{dev_id}=="0x0", ATTR{type}=="1", NAME="usb_tether"
```

```
sudo udevadm control --reload-rules
sudo udevadm trigger
```

Now, your device should show up as "usb_tether". So, you can configure
the interface to request an IPv4 DHCP:

``` /etc/netplan/01-usb-tethering.yaml
etwork:
  version: 2
  ethernets:
    usb_tether:
      dhcp4: true
      dhcp6: true
```

```
sudo netplan apply
```


### Disable IPv6

According to [link](https://itsfoss.com/disable-ipv6-ubuntu-linux/).

```
sudo sysctl -w net.ipv6.conf.all.disable_ipv6=1
sudo sysctl -w net.ipv6.conf.default.disable_ipv6=1
sudo sysctl -w net.ipv6.conf.lo.disable_ipv6=1
```

The configuration can be made persistently by editing/adding `/etc/sysctl.d/`.

## Scripts

### Visualize RNTI Matching

#### Diashow

<details>
<summary>Click to expand</summary>

Visualize the UL traffic of all RNTIs that are left after applying the
pre-filter. Provide `--rnti RNTI` in case you want to filter for a
certain RNTI in the first place.

The output shows how many RNTIs were filtered by the corresponding pre-filter.

Example usage *without* an explicit RNTI:

```
./scripts/visualize_rnti_matching.py --path ".logs.ue/run-<run-date>/rnti_matching/run_<run-date>_traffic_collection.jsonl" diashow
```

</details>

#### Standardize

<details>
<summary>Click to expand</summary>

Print the standardization vector of certain RNTI's traffic.

When you provide a rnti, only the records where the RNTI occurs
(it might be removed by the pre-filter!) is used for standardization.

If you don't provide an RNTI explicitly, it uses the traffic of the
RNTI with the most number of UL occurrences.

Example usage *without* an explicit RNTI:

```
./scripts/visualize_rnti_matching.py --path ".logs.ue/run-<run-date>/rnti_matching/run_<run-date>_traffic_collection.jsonl" standardize
[...]
DEBUG [determine_highest_count_ul_timeline] rnti: 34135 | count: 1873
DEBUG [determine_highest_count_ul_timeline] rnti: 34226 | count: 1186
DEBUG [determine_highest_count_ul_timeline] rnti: 34319 | count: 1166
DEBUG [determine_highest_count_ul_timeline] rnti: 33123 | count: 1619
DEBUG [determine_highest_count_ul_timeline] rnti: 54112 | count: 1529
vec![
    (2735.217, 2362.898),
    (564014.484, 336306.997),
    (65.652, 55.473),
    (327.697, 249.128),
    (428706.906, 643780.033),
    (4422.802, 2244.733),
    (6125.165, 2793.039),
    (156940930.077, 382279093.565)
],
```

Example usage *with* an explicit RNTI:

```
./scripts/visualize_rnti_matching.py --path ".logs.ue/run-<run-date>/rnti_matching/run_<run-date>_traffic_collection.jsonl" standardize --rnti 34226
[...]
DEBUG [determine_highest_count_ul_timeline] rnti: 34226 | count: 1529
vec![
    (2735.217, 2362.898),
    (564014.484, 336306.997),
    (65.652, 55.473),
    (327.697, 249.128),
    (428706.906, 643780.033),
    (4422.802, 2244.733),
    (6125.165, 2793.039),
    (156940930.077, 382279093.565)
],
```

</details>

## Data

Example data and results can be found [here](https://nextcloud.schmidt-systems.eu/s/AYqZDwtWxAeQY8N).
