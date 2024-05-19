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

