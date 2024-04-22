#!/usr/bin/env python3

from flask import Flask, jsonify, request, Response, Request
from marshmallow import Schema, fields, validate
from typing import Callable

app = Flask(__name__)

# Predefined response
cell_data = [
    {"id": 1, "type": "LTE", "earfcn": "Neuron A"},
]

## API parameter validation schemas

class Base(Schema):
    id = fields.Str(required=True)
    execute = fields.Int(required=True)
    core = fields.Str(required=True)
    function = fields.Str(required=True, validate=validate.OneOf(["login", "get"]))

class LoginSchemaValues(Schema):
    username = fields.Str(required=True)
    password = fields.Str(required=True)

class LoginSchema(Base):
    function = fields.Str(required=True, validate=validate.OneOf(["login"]))
    values = fields.List(fields.Nested(LoginSchemaValues, required=True), required=True)

class CellSchemaValues(Schema):
    # Remark: This is not a typo! The value must be "yruo_celluar" - cellular won't work.
    base = fields.Str(required=True, validate=validate.OneOf(["yruo_celluar"]))

class CellSchema(Base):
    function = fields.Str(required=True, validate=validate.OneOf(["get"]))
    values = fields.List(fields.Nested(CellSchemaValues, required=True), required=True)

## API endpoint functions

def login_schema_func(req: Request) -> Response:
    # No actual credentials validation, just positive reponse setting cookies
    positive_response = {
      "id": "1",
      "model": "UR75",
      "pn": "500GL3111111DE0010000000",
      "oem": "0000",
      "rtver": "76.2.0.8-r2",
      "status": 0,
      "result": [
        {
          "ysrole": 4,
          "ystimeout": 1800,
          "ysexpires": 1799
        }
      ]
    }
    resp = jsonify(positive_response)
    resp.set_cookie('loginname', 'admin', path='/')
    resp.set_cookie('td', '643c4d02c565beff7a9978162c5f8433', path='/')
    resp.content_type = "application/json"
    return resp

def cell_schema_func(req: Request) -> Response:
    positive_response = {
      "id": 4,
      "model": "UR75",
      "pn": "500GL3111111DE0010000000",
      "oem": "0000",
      "rtver": "76.2.0.8-r2",
      "status": 0,
      "result": [
        {
          "get": [
            {
              "type": "yruo_cell_status",
              "index": 0,
              "value": {
                "modem": {
                  "modem_status": "Ready",
                  "model": "RM500Q-GL",
                  "version": "RM500QGLABR11A06M4G",
                  "cur_sim": "SIM1",
                  "register": "Registered (Home network)",
                  "imei": "863305040946682",
                  "signal": "31asu (-51dBm)",
                  "imsi": "262011532003967",
                  "iccid": "89490200001888544924",
                  "net_provider": "Telekom.de Telekom.de",
                  "net_type": "LTE",
                  "plmnid": "26201",
                  "lac": "521F",
                  "cellid": "1C17302"
                },
                "network": {
                  "status": "Connected",
                  "ip": "192.0.0.2",
                  "netmask": "255.255.255.224",
                  "gate": "192.0.0.1",
                  "dns": "0.0.0.0",
                  "time": "0 days, 01:23:13",
                  "speed": "RX:     0b, TX:     0b",
                  "ipv6": "2a01:599:906:9d68:70b0:df30:1aea:6b45/64",
                  "gatev6": "2a01:599:906:9d68:71f0:a1cb:1c97:cde2",
                  "dnsv6": "2a01:598:7ff:0:10:74:210:221"
                },
                "statistics": {
                  "sim1": "RX: 0.8 MiB   TX: 14.4 MiB   ALL: 15.2 MiB",
                  "sim2": "RX: 0.0 MiB   TX: 0.0 MiB   ALL: 0.0 MiB"
                },
                "more": {
                  "cqi": "15",
                  "dl": "",
                  "ul": "20 MHz",
                  "sinr": "25",
                  "pcid": "140",
                  "rsrp": "-77dBm",
                  "rsrq": "-8dB",
                  "ecgi": "262010115059002",
                  "earfcn": "1300",
                  "enodeb": "115059"
                }
              }
            }
          ]
        }
      ]
    }
    resp = jsonify(positive_response)
    resp.set_cookie('loginname', 'admin', path='/')
    resp.set_cookie('td', '643c4d02c565beff7a9978162c5f8433', path='/')
    resp.content_type = "application/json"
    return resp

## Map Schema and functions

class SchemaFuncMap:
    def __init__(self, schema: Schema, func: Callable[[Request], Response]):
        self.schema = schema
        self.func = func

SCHEMA_FUNC_MAP = {
    "LoginSchema": SchemaFuncMap(LoginSchema(), login_schema_func),
    "CellSchema": SchemaFuncMap(CellSchema(), cell_schema_func),
}

## API endpoint

@app.route('/cgi')
def cgi():
    all_errors = {}
    for map_name, map in SCHEMA_FUNC_MAP.items():
        errors = map.schema.validate(request.json or {})
        if not errors:
            return map.func(request)
        all_errors[map_name] = errors

    return jsonify({ "No schema could be applied": all_errors })

## Run server

if __name__ == '__main__':
    app.run(port = 8080, debug=True)

