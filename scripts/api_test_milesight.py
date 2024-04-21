#!/usr/bin/env python3

from flask import Flask, jsonify

app = Flask(__name__)

# Predefined response
cell_data = [
    {"id": 1, "type": "LTE", "earfcn": "Neuron A"},
]

@app.route('/api/v1/celldata')
def get_cell_data():
    return jsonify(cell_data)

if __name__ == '__main__':
    app.run(debug=True)
