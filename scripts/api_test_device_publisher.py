#!/usr/bin/env python3

from flask import Flask, jsonify

app = Flask(__name__)

# Predefined response
cell_data = [
    {
        "cid": None,
        "enodeB": 100344,
        "pci": 62,
        "type": "LTE",
        "arfcn": 3350,
        "band": "7",
        "rssi": 110.7,
        "rsrp": 90.7,
        "rsrq": 11.3,
    },
]

@app.route('/api/v1/celldata')
def get_cell_data_all():
    return jsonify(cell_data)

@app.route('/api/v1/celldata/connected/all')
def get_cell_data_connected_all():
    return jsonify(cell_data)

if __name__ == '__main__':
    app.run(debug=True, port=7353)
