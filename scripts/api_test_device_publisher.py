#!/usr/bin/env python3

import argparse
from flask import Flask, jsonify

CELL_DATA_A = {
    "cid": None,
    "enodeB": 100344,
    "pci": 62,
    "type": "LTE",
    "arfcn": 3350,
    "band": "7",
    "rssi": 110.7,
    "rsrp": 90.7,
    "rsrq": 11.3,
}
CELL_DATA_B = {
    "cid": None,
    "enodeB": 105059,
    "pci": 5,
    "type": "LTE",
    "arfcn": 500,
    "band": "1",
    "rssi": 110.7,
    "rsrp": 90.7,
    "rsrq": 11.3,
}
DEFAULT_CELL_DATA_CHOICES = {
    "A": CELL_DATA_A,
    "B": CELL_DATA_B,
}

def default_cell_data() -> str:
    return list(DEFAULT_CELL_DATA_CHOICES.keys())[0]

app = Flask(__name__)
cell_data = []


@app.route('/api/v1/celldata')
def get_cell_data_all():
    return jsonify(cell_data)

@app.route('/api/v1/celldata/connected/all')
def get_cell_data_connected_all():
    return jsonify(cell_data)


def parse_application_args():
    parser = argparse.ArgumentParser(description='Test API to simulate DevicePublisher.')
    parser.add_argument('--cell-data',
                        type=str,
                        choices=list(DEFAULT_CELL_DATA_CHOICES.keys()),
                        default=default_cell_data(),
                        help=f'Column to visualize (default: {default_cell_data()})')
    return parser.parse_args()

def main():
    args = parse_application_args()

    global cell_data
    cell_data = [ DEFAULT_CELL_DATA_CHOICES[args.cell_data] ]

    app.run(debug=True, port=7353)

if __name__ == '__main__':
    main()
