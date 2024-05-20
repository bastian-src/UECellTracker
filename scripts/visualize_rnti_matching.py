#!/usr/bin/env python3

import os
import matplotlib.pyplot as plt
from matplotlib.widgets import Button
from matplotlib.axes import Axes
from datetime import datetime
import pandas as pd
import seaborn as sns
import argparse
import numpy as np
import linecache
import json

# Default Arguments
DEFAULT_DATASET_PATH = './.logs/rnti_matching_pattern_A.jsonl'
DEFAULT_EXPORT_PATH = '.export'
DEFAULT_RNTI = None
DEFAULT_PLOT_FILTERED = True
DEFAULT_EXPORT_RECORDING_INDEX = 17
DEFAULT_RESET_TIMESTAMPS = True
DEFAULT_FILTER_KEEP_REST = False
DEFAULT_EXPORT_PATH = './.export/'

# RNTI Filterting
DCI_THRESHOLD = 0
EMPTY_DCI_RATIO_THRESHOLD = 0.99
SAMPLES_THRESHOLD = 5

MAX_TOTAL_UL_FACTOR = 100.0
MIN_TOTAL_UL_FACTOR = 0.005 # x% of the expected UL traffic
MAX_UL_PER_DCI_THRESHOLD = 5_000_000
MIN_OCCURENCES_FACTOR = 0.005

# Plotting
PLOT_SCATTER_MARKER_SIZE = 10
MARKERS = ['o', 's', '^', 'd', 'p', 'h', '*', '>', 'v', '<', 'x']

sns.set(style="darkgrid")

##########
# Helpers
##########

def print_debug(msg: str):
    print(msg)

def print_info(msg: str):
    print(msg)


def count_lines(file_path: str) -> int:
    with open(file_path, 'r') as file:
        return sum(1 for _ in file)


def read_single_dataset(file_path: str, line_number: int) -> dict:
    try:
        return json.loads(linecache.getline(file_path, line_number).strip())
    except Exception as e:
        raise Exception(f"An error occured reading dataset at {file_path}:{line_number}\n{e}")


def merge_ue_traffic(dict_a, dict_b):
    for key, value in dict_b.items():
        if key in dict_a:
            dict_a[key]['ul_bytes'] += value['ul_bytes']
            dict_a[key]['dl_bytes'] += value['dl_bytes']
        else:
            dict_a[key] = {'ul_bytes': value['ul_bytes'], 'dl_bytes': value['dl_bytes']}

    return dict_a


def move_column_to_front(df: pd.DataFrame, specific_column: str) -> pd.DataFrame:
    """
    Move a specific column to the front of a Pandas DataFrame.

    Parameters:
    - df (pd.DataFrame): The input DataFrame.
    - specific_column (str): The column name to move to the front.

    Returns:
    - pd.DataFrame: DataFrame with the specified column moved to the front.
    """
    cols = df.columns.tolist()
    cols.insert(0, cols.pop(cols.index(specific_column)))
    cols_df = df[cols]

    if isinstance(cols_df, pd.DataFrame):
        return cols_df

    raise Exception("Should not happen, move_column_to_front tried to return something else than a pd.DataFrame")


def save_plot(file_path):
    plt.tight_layout()
    plt.savefig(file_path)
    print_debug(f"Saved file: {file_path}")


def recording_to_pandas(recording) -> pd.DataFrame:
    """
    Converts a recording of uplink (UL) bytes data from multiple RNTIs into a Pandas DataFrame.

    Args:
        recording (dict): A dictionary containing the recording data. The structure of the dictionary is:
                          Example:
                          { 'recordings': [
                                  ( { 1612345600000000: {'ul_bytes': 100, 'dl_bytes': 200}, }, 'RNTI_A'),
                                  ...
                              ]
                          }

    Returns:
        pd.DataFrame: A DataFrame with timestamps as the index and RNTIs as columns. Each cell contains
                      the 'ul_bytes' value for the corresponding timestamp and RNTI. If an RNTI did not
                      send anything at a specific timestamp, the cell will contain NaN.

    Note:
        - The function assumes that the 'recordings' key is always present in the recording dictionary.
        - Timestamps are assumed to be in epoch time format in microseconds.
        - Actual timestamps are masqueraded
    """
    flatten_data = {}
    for ue_traffic, rnti in recording['recordings']:
        for timestamp, values in ue_traffic.items():
            converted_timestamp = np.datetime64(int(timestamp), 'us')
            if converted_timestamp not in flatten_data:
                flatten_data[converted_timestamp] = {}
            flatten_data[converted_timestamp][rnti] = values['ul_bytes']

    df = pd.DataFrame.from_dict(flatten_data, orient='index')
    df.sort_index(inplace=True)

    df.index = pd.to_datetime(df.index)
    # Reset the index to start from 0:00
    df.index = (df.index - df.index[0]).astype('timedelta64[us]')

    return df

##########
# Diashow
##########

def diashow(settings):
    data = [None for _ in range(count_lines(settings.path))]
    print(f"Number of datasets in the file: {len(data)}")

    _, ax = plt.subplots()
    tracker = IndexTracker(ax, data, settings)

    axprev = plt.axes([0.7, 0.01, 0.1, 0.075])
    axnext = plt.axes([0.81, 0.01, 0.1, 0.075])
    bnext = Button(axnext, 'Next')
    bnext.on_clicked(tracker.next)
    bprev = Button(axprev, 'Previous')
    bprev.on_clicked(tracker.prev)

    plt.show()


class FilteredRecording:
    def __init__(self):
        self.filtered_df: pd.DataFrame = pd.DataFrame()
        self.nof_empty_dci: int = 0
        self.nof_total_dci: int = 0
        self.expected_ul_bytes: int = 0
        self.expected_nof_packets: int = 0
        self.skipped_max_total_ul: int = 0
        self.skipped_max_per_dci_ul: int = 0
        self.skipped_min_exp_ul: int = 0
        self.skipped_min_count: int = 0
        self.skipped_median_zero: int = 0
        self.skipped_not_target_rnti: int = 0


def filter_dataset(settings, raw_dataset) -> FilteredRecording:
    if settings.filter_keep_rest:
        return filter_dataset_slow(settings, raw_dataset)
    else:
        return filter_dataset_fast(settings, raw_dataset)


def filter_dataset_fast(settings, raw_dataset) -> FilteredRecording:
    cell_traffic = raw_dataset['cell_traffic']
    if len(cell_traffic) > 1:
        print(f"!!! More than one cell: {len(cell_traffic)}. Using only the first one.")

    _, cell_data = next(iter((cell_traffic.items())))

    result: FilteredRecording = FilteredRecording()
    result.nof_empty_dci = cell_data['nof_empty_dci']
    result.nof_total_dci = cell_data['nof_total_dci']
    result.expected_ul_bytes = raw_dataset['traffic_pattern_features']['total_ul_bytes']
    result.expected_nof_packets = raw_dataset['traffic_pattern_features']['nof_packets']

    min_total_ul = result.expected_ul_bytes * MIN_TOTAL_UL_FACTOR
    max_total_ul = result.expected_ul_bytes * MAX_TOTAL_UL_FACTOR
    min_count = result.expected_nof_packets * MIN_OCCURENCES_FACTOR

    # Start checks
    if result.nof_total_dci <= DCI_THRESHOLD:
        raise Exception("nof_total_dci <= DCI_THRESHOLD")
    if result.nof_empty_dci > EMPTY_DCI_RATIO_THRESHOLD * result.nof_total_dci:
        raise Exception("nof_empty_dci > EMPTY_DCI_RATIO_THRESHOLD * nof_total_dci")

    for _, cell_data in cell_traffic.items():
        flattened: dict = {}
        for rnti, ue_data in cell_data['traffic'].items():
            if hasattr(settings, 'rnti'):
                if settings.rnti is not None and rnti != settings.target_rnti:
                    result.skipped_not_target_rnti += 1
                    continue

            if ue_data['total_ul_bytes'] > max_total_ul:
                result.skipped_max_total_ul += 1
                # print(f"RNTI_UL_THRESHOLD: {ue_data['total_ul_bytes']}")
                continue

            if ue_data['total_ul_bytes']  < min_total_ul:
                result.skipped_min_exp_ul += 1
                # print(f"MIN_EXPECTED_UL_THRESHOLD: {ue_data['total_ul_bytes']}")
                continue

            if len(ue_data['traffic']) < min_count:
                result.skipped_min_count += 1
                # print(f"MIN_OCCURENCES_THRESHOLD: {len(ue_data['traffic'])}")
                continue

            skip_rnti = False
            for _, tx_data in ue_data['traffic'].items():
                if tx_data['ul_bytes'] > MAX_UL_PER_DCI_THRESHOLD:
                    result.skipped_max_per_dci_ul += 1
                    skip_rnti = True
                    break
            if skip_rnti:
                result.skipped_max_per_dci_ul += 1
                # print("MAX_UL_BYTES_PER_DCI")
                continue

            if calculate_median(ue_data) <= 0:
                result.skipped_median_zero += 1
                continue

            for timestamp, values in ue_data['traffic'].items():
                converted_timestamp = np.datetime64(int(timestamp), 'us')
                if converted_timestamp not in flattened:
                    flattened[converted_timestamp] = {}
                flattened[converted_timestamp][rnti] = values['ul_bytes']

            all_df = pd.DataFrame.from_dict(flattened, orient='index')
            all_df.sort_index(inplace=True)

            all_df.index = pd.to_datetime(all_df.index)
            if settings.reset_timestamps:
                # Reset the index to start from 0:00
                all_df.index = (all_df.index - all_df.index[0]).astype('timedelta64[us]')

            result.filtered_df = all_df
            

    print("Skipped RNTIs during filtering:")
    print(f"    MAX TOTAL UL: \t {result.skipped_max_total_ul} \t({max_total_ul})")
    print(f"    MIN TOTAL UL: \t {result.skipped_min_exp_ul} \t({min_total_ul})")
    print(f"    MAX UL/DCI: \t {result.skipped_max_per_dci_ul}")
    print(f"    MIN OCCURE: \t {result.skipped_min_count} \t({min_count})")
    print(f"    MEDIAN UL:  \t {result.skipped_median_zero}")
    if hasattr(settings, 'rnti') and settings.rnti is not None:
        print(f"    TARGET RNTI: \t {result.skipped_not_target_rnti}")

    print(f"Nof valid RNTIs: {result.filtered_df.shape[1]}")
    if result.filtered_df.shape[1] <= 0:
        raise Exception("No RNTIs left after filtering!!")
    
    return result


def filter_dataset_slow(settings, raw_dataset) -> FilteredRecording:
    cell_traffic = raw_dataset['cell_traffic']
    if len(cell_traffic) > 1:
        print(f"!!! More than one cell: {len(cell_traffic)}. Using only the first one.")

    _, cell_data = next(iter((cell_traffic.items())))

    result: FilteredRecording = FilteredRecording()
    result.nof_empty_dci = cell_data['nof_empty_dci']
    result.nof_total_dci = cell_data['nof_total_dci']
    result.expected_ul_bytes = raw_dataset['expected_ul_bytes']
    result.expected_nof_packets = raw_dataset['expected_nof_packets']

    for _, cell_data in cell_traffic.items():
        flattened: dict = {}
        for rnti, ue_data in cell_data['traffic'].items():
            for timestamp, values in ue_data['traffic'].items():
                converted_timestamp = np.datetime64(int(timestamp), 'us')
                if converted_timestamp not in flattened:
                    flattened[converted_timestamp] = {}
                flattened[converted_timestamp][rnti] = values['ul_bytes']

        all_df = pd.DataFrame.from_dict(flattened, orient='index')
        all_df.sort_index(inplace=True)

        all_df.index = pd.to_datetime(all_df.index)
        if settings.reset_timestamps:
            # Reset the index to start from 0:00
            all_df.index = (all_df.index - all_df.index[0]).astype('timedelta64[us]')

        result.filtered_df = all_df

    print(f"Nof RNTIs: {result.filtered_df.shape[1]}")
    if result.filtered_df.shape[1] <= 0:
        raise Exception("No RNTIs left after filtering!!")

    return result


def convert_timestamp(timestamp):
    dt = datetime.fromtimestamp(timestamp / 1_000_000.0)
    return dt.strftime('%Y-%m-%d %H:%M:%S.%f')[:-3]


def calculate_median(ue_traffic):
    numbers = [tx_data['ul_bytes'] for _, tx_data in ue_traffic['traffic'].items()]
    sorted_list = sorted(numbers)
    length = len(sorted_list)

    if length % 2 == 0:
        # If the length is even, return the average of the two middle elements
        mid1 = sorted_list[length // 2 - 1]
        mid2 = sorted_list[length // 2]
        median = (mid1 + mid2) / 2
    else:
        # If the length is odd, return the middle element
        median = sorted_list[length // 2]

    return median


def plot_data(ax, dataset, index, settings):
    ax.clear()
    rnti_count = {}
    all_timestamps = []

    if settings.plot_filtered and len(dataset[index]['removed_by_filter']) > 0:
        ue_traffic = dataset[index]['removed_by_filter']

        timestamps = np.array([np.datetime64(int(ts), 'us') for ts in sorted(list(ue_traffic.keys()))])
        ul_bytes = [ue_traffic[ts]['ul_bytes'] for ts in sorted(list(ue_traffic.keys()))]

        ax.scatter(timestamps, ul_bytes, color='grey', alpha=0.2, label='Filtered RNTIs')


    for ue_traffic, rnti in dataset[index]['recordings']:
        # Extract timestamps and UL bytes
        timestamps = np.array([np.datetime64(int(ts), 'us') for ts in sorted(list(ue_traffic.keys()))])
        all_timestamps.extend(timestamps)

        ul_bytes = [ue_traffic[ts]['ul_bytes'] for ts in sorted(list(ue_traffic.keys()))]

        rnti_count[rnti] = len(timestamps)
        # Plot UL bytes for the RNTI
        ax.scatter(timestamps, ul_bytes, label=f'RNTI: {rnti}', s=PLOT_SCATTER_MARKER_SIZE)

    max_key = max(rnti_count, key=rnti_count.get)
    max_value = rnti_count[max_key]

    # y-axis limit
    # ax.set_ylim(0, 2000)

    nof_total_dci = dataset[index]['nof_total_dci']
    nof_empty_dci = dataset[index]['nof_empty_dci']

    ax.set_xlabel('Timestamp')
    ax.set_ylabel('UL Bytes')
    ax.set_title(f'UL Traffic Pattern | Dataset: {index + 1}/{len(dataset)} | Empty DCI {nof_empty_dci}/{nof_total_dci} | Most occurring RNTI: [{max_key}]: {max_value}')
    # ax.grid(True, which='both', axis='y', linestyle='--', linewidth=0.5, color='gray', alpha=0.7)

    # Rotate x-axis labels
    for label in ax.get_xticklabels():
        label.set_rotation(45)
        label.set_ha('right')

    ax.legend()

    plt.draw()


class IndexTracker:
    def __init__(self, ax, data: list, settings):
        self.ax = ax
        self.data = data # : list of FilteredRecording
        self.settings = settings
        self.index = 0
        if self.check_data(self.index):
            self.plot()

    def plot(self):
        self.ax.clear()
        df_kbps = self.data[self.index].filtered_df.resample('1s').median().mul(8 / 1_000)
        plot_df(plot_pandas_line, df_kbps, axes=self.ax)

    def check_data(self, file_index) -> bool:
        if isinstance(self.data[file_index], FilteredRecording):
            return True
        elif self.data[self.index] == None:
            # Read dataset
            try:
                raw_data = read_single_dataset(self.settings.path, self.index + 1)
                filtered_data = filter_dataset(self.settings, raw_data)
                if filtered_data is not None:
                    self.data[file_index] = filtered_data
                    print(f"Successfully loaded dataset {self.settings.path}:{file_index}")
                    return True
            except Exception as e:
                print(f"Dataset not plottable: {e}")

        return False

    def next(self, _):
        if count_lines(self.settings.path) != len(self.data):
            self.data = [None for _ in range(count_lines(self.settings.path))]
        self.index = (self.index + 1) % len(self.data)
        if self.check_data(self.index):
            self.plot()

    def prev(self, _):
        if count_lines(self.settings.path) != len(self.data):
            self.data = [None for _ in range(count_lines(self.settings.path))]
        self.index = (self.index - 1) % len(self.data)
        if self.check_data(self.index):
            self.plot()


##########
# Standardize
##########

def standardize(settings):
    all_recordings = read_all_recordings(settings)
    print(f"DEBUG len(all_data) {len(all_recordings)}")
    ul_timeline_matrix = np.zeros((len(all_recordings), 3))
    dci_time_deltas_matrix = np.zeros((len(all_recordings), 3))
    count_vec = np.zeros((len(all_recordings), 1))
    total_ul_vec = np.zeros((len(all_recordings), 1))
    for (index, recordings) in enumerate(all_recordings):
        (ul_timeline, dci_time_deltas, count, total_ul_bytes) = determine_highest_count_ul_timeline(recordings)
        count_vec[index] = count
        total_ul_vec[index] = total_ul_bytes

        ul_timeline_matrix[index, 0] = np.median(ul_timeline)
        ul_timeline_matrix[index, 1] = np.mean(ul_timeline)
        ul_timeline_matrix[index, 2] = np.var(ul_timeline)

        dci_time_deltas_matrix[index, 0] = np.median(dci_time_deltas)
        dci_time_deltas_matrix[index, 1] = np.mean(dci_time_deltas)
        dci_time_deltas_matrix[index, 2] = np.var(dci_time_deltas)

    std_count = (np.mean(count_vec), np.std(count_vec))
    std_total_ul = (np.mean(total_ul_vec), np.std(total_ul_vec))

    std_ul_timeline_median = (
            np.mean(ul_timeline_matrix[:, 0]),
            np.std(ul_timeline_matrix[:, 0])
    )

    ul_timeline_mean = (
            np.mean(ul_timeline_matrix[:, 1]),
            np.std(ul_timeline_matrix[:, 1])
    )

    ul_timeline_variance = (
            np.mean(ul_timeline_matrix[:, 2]),
            np.std(ul_timeline_matrix[:, 2])
    )

    dci_time_deltas_median = (
            np.mean(dci_time_deltas_matrix[:, 0]),
            np.std(dci_time_deltas_matrix[:, 0])
    )

    dci_time_deltas_mean = (
            np.mean(dci_time_deltas_matrix[:, 1]),
            np.std(dci_time_deltas_matrix[:, 1])
    )

    dci_time_deltas_variance = (
            np.mean(dci_time_deltas_matrix[:, 2]),
            np.std(dci_time_deltas_matrix[:, 2])
    )

    # Print the values in Rust tuple format with three decimal places
    print("vec![")
    print(f"    ({std_count[0]:.3f}, {std_count[1]:.3f}),")
    print(f"    ({std_total_ul[0]:.3f}, {std_total_ul[1]:.3f}),")
    print(f"    ({std_ul_timeline_median[0]:.3f}, {std_ul_timeline_median[1]:.3f}),")
    print(f"    ({ul_timeline_mean[0]:.3f}, {ul_timeline_mean[1]:.3f}),")
    print(f"    ({ul_timeline_variance[0]:.3f}, {ul_timeline_variance[1]:.3f}),")
    print(f"    ({dci_time_deltas_median[0]:.3f}, {dci_time_deltas_median[1]:.3f}),")
    print(f"    ({dci_time_deltas_mean[0]:.3f}, {dci_time_deltas_mean[1]:.3f}),")
    print(f"    ({dci_time_deltas_variance[0]:.3f}, {dci_time_deltas_variance[1]:.3f})")
    print("],")


def read_all_recordings(settings):
    all_runs = []
    for line_number in range(1, count_lines(settings.path)):
        raw_data = read_single_dataset(settings.path, line_number)
        try:
            result = filter_dataset(settings, raw_data)
            if result is not None:
                all_runs.append(result.filtered_df)
        except:
            pass

    return all_runs


def determine_highest_count_ul_timeline(df):
    rnti = "11223"
    target_traffic: pd.DataFrame = pd.DataFrame()

    if not rnti in df.columns:
        rnti = df.count().idxmax()
    target_traffic = df[rnti].dropna()

    ul_timeline = target_traffic.values
    count = target_traffic.count()
    total_ul_bytes = target_traffic.sum()
    index_numpy = target_traffic.index.to_numpy()
    dci_time_deltas = (index_numpy[1:] - index_numpy[:-1]).astype(int)

    print_debug(f"DEBUG [determine_highest_count_ul_timeline] rnti: {rnti} | count: {count}")

    return (ul_timeline, dci_time_deltas, count, total_ul_bytes)


##########
# Exports
##########

def export(settings):
    dir_name, base_name = os.path.split(settings.path)
    base_name_no_ext, _ = os.path.splitext(base_name)
    export_base_path = os.path.join(settings.export_path,
                                    dir_name.split('/', 1)[-1],
                                    base_name_no_ext)

    raw_recording = read_single_dataset(settings.path, settings.recording_index)
    filtered_data = filter_dataset(settings, raw_recording)

    df = move_column_to_front(filtered_data.filtered_df, '11985')
    df_kbps = df.resample('1s').median().mul(8 / 1_000)

    # Replace the first directory and change the file extension
    os.makedirs(os.path.dirname(export_base_path), exist_ok=True)

    print_info(f"Exporting files to: {export_base_path}*")

    ###
    # Show filtered (resample)
    ###
    plot_df(plot_pandas_line, df_kbps)
    xlim = plt.xlim()
    ylim = plt.ylim()
    save_plot(export_base_path + "_filtered.png")
    save_plot(export_base_path + "_filtered.pdf")
    save_plot(export_base_path + "_filtered.svg")
    plt.close()

    ###
    # Show Single one
    ###
    df_single_rnti = df_kbps[['11985']]

    plot_df(plot_pandas_line, df_single_rnti)
    plt.xlim(xlim)
    plt.ylim(ylim)
    save_plot(export_base_path + "_single.png")
    save_plot(export_base_path + "_single.pdf")
    save_plot(export_base_path + "_single.svg")
    plt.close()

    ###
    # Show all data
    ###

    print_info( "  [ ] Reading the whole dataset without filtering..")
    df_all: pd.DataFrame = filter_dataset_slow(settings, raw_recording).filtered_df
    df_all = move_column_to_front(df_all, '11985')
    df_all_kbps: pd.DataFrame = df_all.mul(8 / 1_000)
    print_info( "  [x] Reading the whole dataset without filtering..")

    print_info(f"  [ ] Plotting unfiltered {df_all_kbps.shape[0]}x{df_all_kbps.shape[1]}..")
    plot_df(plot_pandas_line, df_all_kbps, legend=False)
    print_info(f"  [x] Plotting unfiltered {df_all_kbps.shape[0]}x{df_all_kbps.shape[1]}..")

    save_plot(export_base_path + "_unfiltered.png")
    save_plot(export_base_path + "_unfiltered.pdf")
    save_plot(export_base_path + "_unfiltered.svg")
    plt.show()
    plt.close()


    # plot_pandas_scatter(pandas_recording)
    # plot_pandas_resample_scatter(pandas_recording)
    # plot_pandas_hist(pandas_recording)
    # plot_pandas_hist_log(pandas_recording, freedman_diaconis_bins)
    # plot_pandas_hist_log(pandas_recording, sturges_bins)
    # plot_pandas_hist_log(pandas_recording, scott_bins)
    # plot_pandas_hist_log(pandas_recording, log_bins)

    # plot_raw(settings, recording)
    # plot_basic_filtered(settings, filtered_recording)


def plot_df(func, df: pd.DataFrame, axes=None, legend=True):
    ax: Axes = axes

    if ax is None:
        _, ax = plt.subplots()

    func(ax, df)

    # ax.set_title('Scatter Plot of UL Bytes over Time')
    # ax.tick_params(axis='x', rotation=45)
    ax.set_xlabel('Timestamp (seconds)', fontsize=28)
    ax.set_ylabel('UL Traffic (kbit/s)', fontsize=28)
    ax.tick_params(axis='x', labelsize=24)
    ax.tick_params(axis='y', labelsize=24)

    if legend:
        ax.legend(fontsize=18)

    if ax is None:
        plt.show()
    else:
        plt.draw()


def plot_pandas_scatter(ax, df: pd.DataFrame):
    for i, column in enumerate(df.columns):
        ax.scatter(df.index.total_seconds(), df[column], label=column, marker=MARKERS[i % len(MARKERS)], s=PLOT_SCATTER_MARKER_SIZE)



def plot_pandas_line(ax, df):
    for i, column in enumerate(df.columns):
        x_values = df.index.total_seconds().values  # Convert index to NumPy array
        y_values = df[column].values
        ax.plot(x_values, y_values, marker=MARKERS[i % len(MARKERS)], label=column)


def log_bins(data):
    return np.logspace(np.log10(20), np.log10(data.max()), num=50)


def plot_pandas_hist_log(ax, df, bin_func=log_bins):
    all_ul_bytes = df.stack().values
    bins = bin_func(all_ul_bytes)

    # Create logarithmic bins
    for column in df.columns:
        ax.hist(df[column], bins=bins, edgecolor='k', alpha=0.7, label=column)

    ax.set_xscale('symlog')
    # ax.set_title('Histogram of UL Bytes')


def plot_pandas_hist(ax, df):

    df.plot(kind='hist', bins=50, alpha=0.5, ax=ax)
    ax.set_xlabel('UL Bytes')
    ax.set_ylabel('Frequency')
    # ax.set_title('Histogram of UL Bytes')


def plot_basic_filtered(settings, recording):

    rnti_count = {}
    all_timestamps = []
    _, ax = plt.subplots(figsize=(12, 6))

    if settings.plot_filtered and len(recording['removed_by_filter']) > 0:
        ue_traffic = recording['removed_by_filter']

        timestamps = np.array([np.datetime64(int(ts), 'us') for ts in sorted(list(ue_traffic.keys()))])
        ul_bytes = [ue_traffic[ts]['ul_bytes'] for ts in sorted(list(ue_traffic.keys()))]

        ax.scatter(timestamps, ul_bytes, color='grey', alpha=0.2, label='Filtered RNTIs')


    for ue_traffic, rnti in recording['recordings']:
        # Extract timestamps and UL bytes
        timestamps = np.array([np.datetime64(int(ts), 'us') for ts in sorted(list(ue_traffic.keys()))])
        all_timestamps.extend(timestamps)

        ul_bytes = [ue_traffic[ts]['ul_bytes'] for ts in sorted(list(ue_traffic.keys()))]

        rnti_count[rnti] = len(timestamps)
        # Plot UL bytes for the RNTI
        ax.scatter(timestamps, ul_bytes, label=f'RNTI: {rnti}', s=PLOT_SCATTER_MARKER_SIZE)

    max_key = max(rnti_count, key=rnti_count.get)
    max_value = rnti_count[max_key]

    ax.set_yscale('symlog', linthresh=5000, linscale=1)

    # formatter = ScalarFormatter(useMathText=True)
    # formatter.set_scientific(True)
    # ax.yaxis.set_major_formatter(formatter)
    # ax.yaxis.set_major_locator(LogLocator(base=10.0, subs='auto', numticks=5))
    # ax.yaxis.set_major_locator(SymmetricalLogLocator(base=10, linthresh=20000, subs=[1.0, 2.0, 5.0]))  # Log part above linthresh

    # y-axis limit
    # ax.set_ylim(0, 2000)

    ax.set_xlabel('Timestamp')
    ax.set_ylabel('UL Bytes')
    ax.set_title(f"Basic filtered UL traffic | Number of RNTIs: {len(recording['recordings'])}")
    ax.grid(True, which='both', axis='y', linestyle='--', linewidth=0.5, color='gray', alpha=0.7)

    # Rotate x-axis labels
    for label in ax.get_xticklabels():
        label.set_rotation(45)
        label.set_ha('right')

    plt.draw()
    plt.tight_layout()

    plt.show()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description='Display UL traffic patterns from a dataset.')
    parser.add_argument('--path',
                        type=str,
                        default=DEFAULT_DATASET_PATH,
                        help=f'Path to the dataset file (default: {DEFAULT_DATASET_PATH})')
    parser.add_argument('--reset-timestamps',
                        type=bool,
                        default=DEFAULT_RESET_TIMESTAMPS,
                        help=f'Reset timestamps to 00:00 (default: {DEFAULT_RESET_TIMESTAMPS})')
    parser.add_argument('--filter-keep-rest',
                        type=bool,
                        default=DEFAULT_FILTER_KEEP_REST,
                        help=f'If one wants to keep the rest, the filtering takes significantly longer (default: {DEFAULT_FILTER_KEEP_REST})')

    subparsers = parser.add_subparsers(dest='command', required=True)

    # diashow subcommand
    parser_diashow = subparsers.add_parser('diashow', help='Run diashow mode')
    parser_diashow.add_argument('--rnti',
                                type=str,
                                default=None,
                                help='Display only a single RNTI')
    parser_diashow.add_argument('--plot-filtered',
                                type=bool,
                                default=DEFAULT_PLOT_FILTERED,
                                help='Display filtered RNTIs in the plot (default: {DEFAULT_PLOT_FILTERED})')

    # standardize subcommand
    parser_standardize = subparsers.add_parser('standardize', help='Run standardize mode')

    # export subcommand
    parser_export = subparsers.add_parser('export', help='Run export mode')
    parser_export.add_argument('--export-path',
                               type=str,
                               default=DEFAULT_EXPORT_PATH,
                               help=f'Base path to export images to (default: {DEFAULT_EXPORT_PATH})')
    parser_export.add_argument('--recording-index',
                               type=int,
                               default=DEFAULT_EXPORT_RECORDING_INDEX,
                               help='Index of the recording in the dataset (default: {DEFAULT_EXPORT_RECORDING_INDEX})')
    parser_export.add_argument('--plot-filtered',
                               type=bool,
                               default=DEFAULT_PLOT_FILTERED,
                               help='Display filtered RNTIs in the plot (default: {DEFAULT_PLOT_FILTERED})')

    args = parser.parse_args()

    if args.command == 'diashow':
        diashow(args)
    elif args.command == 'standardize':
        standardize(args)
    elif args.command == 'export':
        export(args)
