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

## Data

Example data and results can be found [here](https://nextcloud.schmidt-systems.eu/s/AYqZDwtWxAeQY8N).
