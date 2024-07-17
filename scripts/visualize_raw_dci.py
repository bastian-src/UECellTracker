#!/usr/bin/env python3

import os
import glob
from typing import Optional
import matplotlib.pyplot as plt
from matplotlib.axes import Axes
# from datetime import datetime
import pandas as pd
import seaborn as sns
import argparse
import numpy as np
import pyarrow as pa
import pyarrow.ipc as ipc
import pyarrow.feather as feather


# Default Arguments
DEFAULT_EXPORT_PATH = '.export'
DEFAULT_RNTI = None
DEFAULT_PLOT_FILTERED = True
DEFAULT_EXPORT_RECORDING_INDEX = 17
DEFAULT_RESET_TIMESTAMPS = True
DEFAULT_FILTER_KEEP_REST = False
DEFAULT_EXPORT_PATH = './.export/'
DEFAULT_DIASHOW_KBPS = False

DEFAULT_COLUMN = 'dl_tbs'
DEFAULT_PLOT_COMBINE = False

# Plotting
DEFAULT_PLOT_PATH = '.logs/processed.dci/*.arrow'
PLOT_SCATTER_MARKER_SIZE = 10
MARKERS = ['o', 's', '^', 'd', 'p', 'h', '*', '>', 'v', '<', 'x']

# Processing
DEFAULT_PROCESS_PATH = './.logs/dci/*.arrow'
DEFAULT_PROCESS_OUTPUT_DIR = './.logs/processed.dci/'

sns.set(style="darkgrid")


def plot_pandas_scatter(ax: Axes, df: pd.DataFrame):
    if not isinstance(df.index, pd.DatetimeIndex):
        raise ValueError("DataFrame index must be a DatetimeIndex")

    seconds = df.index.map(lambda x: x.total_seconds())

    for i, column in enumerate(df.columns):
        ax.scatter(seconds, df[column], label=column, marker=MARKERS[i % len(MARKERS)], s=PLOT_SCATTER_MARKER_SIZE)


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

DEFAULT_DIASHOW_PLOT_TYPE = 'scatter'
DEFAULT_DIASHOW_PLOT_TYPE_CHOICES = {
    'scatter': plot_pandas_scatter,
    'line': plot_pandas_line,
    'hist': plot_pandas_hist,
}

def plot_cdf(ax, series, bins=100):
    x = np.array(series.dropna().sort_values())
    x_mean = [np.array(series.mean())] * len(x)
    x_median = [np.array(series.median())] * len(x)
    y = np.linspace(0, 1, len(x))
    y_mean = np.interp(x_mean, x, y)[0]
    y_median = np.interp(x_median, x, y)[0]
    x_90 = x[np.searchsorted(y, 0.9, side="right")]
    ax.plot(x, y, color="tab:blue")
    ax.set_ylabel("CDF")
    ax.set_ylim([0, 1])
    ax.set_xlim([0, max(x)])

    ax.grid(axis="y", linestyle="--")
    ax.grid(axis="x", linestyle="--")


def create_subdirs(file_path):
    directory = os.path.dirname(file_path)
    os.makedirs(directory, exist_ok=True)


def read_arrow(path) -> pd.DataFrame:
    with pa.memory_map(path, 'r') as source:
        reader = ipc.RecordBatchFileReader(source)
        table = reader.read_all()
    df = table.to_pandas()
    return df


def write_arrow(df, file_path):
    with open(file_path, 'wb') as f:
        feather.write_feather(df, f)


def get_sum(rnti_list: list, item_name):
    try:
        return sum(item[item_name] for item in rnti_list)
    except:
        summed = 0
        for item in rnti_list:
            summed += float(item[item_name])
        return summed


def unpack_rnti_list(row):
    if row['nof_rnti'] == 0:
        return row

    # Manually normalize the 'rnti_list' without using pd.json_normalize
    rnti_list = row['rnti_list']
    dl_tbs_bit_sum = get_sum(rnti_list, 'dl_tbs_bit')
    dl_prb_sum = get_sum(rnti_list, 'dl_prb')
    dl_no_tbs_prb_sum = get_sum(rnti_list, 'dl_no_tbs_prb')

    ul_tbs_bit_sum = get_sum(rnti_list, 'ul_tbs_bit')
    ul_prb_sum = get_sum(rnti_list, 'ul_prb')
    ul_no_tbs_prb_sum = get_sum(rnti_list, 'ul_no_tbs_prb')

    # Calculate the max_ratio for each column
    dl_tbs_bit_max_ratio = max(item['dl_tbs_bit'] / dl_tbs_bit_sum if dl_tbs_bit_sum > 0 else np.NaN for item in rnti_list)
    dl_prb_max_ratio = max(item['dl_prb'] / dl_prb_sum if dl_prb_sum > 0 else np.NaN for item in rnti_list)
    dl_no_tbs_prb_max_ratio = max(item['dl_no_tbs_prb'] / dl_no_tbs_prb_sum if dl_no_tbs_prb_sum > 0 else np.NaN for item in rnti_list)
    ul_tbs_bit_max_ratio = max(item['ul_tbs_bit'] / ul_tbs_bit_sum if ul_tbs_bit_sum > 0 else np.NaN for item in rnti_list)
    ul_prb_max_ratio = max(item['ul_prb'] / ul_prb_sum if ul_prb_sum > 0 else np.NaN for item in rnti_list)
    ul_no_tbs_prb_max_ratio = max(item['ul_no_tbs_prb'] / ul_no_tbs_prb_sum if ul_no_tbs_prb_sum > 0 else np.NaN for item in rnti_list)

    # Combine the sums and max_ratios into the result
    row.dl_tbs_bit = dl_tbs_bit_sum
    row.dl_prb = dl_prb_sum
    row.dl_no_tbs_prb = dl_no_tbs_prb_sum

    row.ul_tbs_bit = ul_tbs_bit_sum
    row.ul_prb = ul_prb_sum
    row.ul_no_tbs_prb = ul_no_tbs_prb_sum

    row.dl_tbs_bit_max_ratio = dl_tbs_bit_max_ratio
    row.dl_prb_max_ratio = dl_prb_max_ratio
    row.dl_no_tbs_prb_max_ratio = dl_no_tbs_prb_max_ratio

    row.ul_tbs_bit_max_ratio = ul_tbs_bit_max_ratio
    row.ul_prb_max_ratio = ul_prb_max_ratio
    row.ul_no_tbs_prb_max_ratio = ul_no_tbs_prb_max_ratio

    return row
    


def apply_row_func(df, row_func) -> pd.DataFrame:
    return df.apply(row_func, axis=1)


def add_nan_columns(df) -> pd.DataFrame:
    return df.assign(
        dl_tbs_bit=np.NaN,
        dl_prb=np.NaN,
        dl_no_tbs_prb=np.NaN,
        ul_tbs_bit=np.NaN,
        ul_prb=np.NaN,
        ul_no_tbs_prb=np.NaN,
        dl_prb_max_ratio=np.NaN,
        dl_tbs_bit_max_ratio=np.NaN,
        dl_no_tbs_max_ratio=np.NaN,
        ul_prb_max_ratio=np.NaN,
        ul_tbs_bit_max_ratio=np.NaN,
        ul_no_tbs_max_ratio=np.NaN,
    )


def convert_to_datetime(timestamp):
    return np.datetime64(int(timestamp), 'us')


def transform_df_table(df_table) -> pd.DataFrame:
    nan_df = add_nan_columns(df_table)
    processed_df = apply_row_func(nan_df, unpack_rnti_list)
    processed_df.timestamp = pd.to_datetime(processed_df.timestamp.apply(convert_to_datetime))
    processed_df.set_index('timestamp', inplace=True)

    # TODO: convert timetamps properly:
    # converted_timestamp = np.datetime64(int(timestamp), 'us')
    # df.index = pd.to_datetime(df.index)
    # df.index = (df.index - df.index[0]).astype('timedelta64[us]')
    # if settings.reset_timestamps:
    #     # Reset the index to start from 0:00
    #     all_df.index = (all_df.index - all_df.index[0]).astype('timedelta64[us]')

    return processed_df


def print_info(msg: str):
    print(msg)


def get_file_paths(path: str) -> list:
    """
    Check whether the provided path is a pattern that matches a single file or multiple files
    and return the corresponding list of individual file paths.

    :param path: The file path or pattern.
    :return: A list of individual file paths.
    """
    matched_files = glob.glob(path)

    if not matched_files:
        raise FileNotFoundError(f"No files matched the pattern: {path}")

    return matched_files


def determine_run_from_file_paths(settings, file_paths: list[str]) -> str:
    run_ids = [path.split('/')[2].split('_cell_data')[0] for path in file_paths]
    # Just take the first run id as new name
    first_unique_id = list(set(run_ids))[0]
    return f"{settings.output_dir}{first_unique_id}_cell_data_combined.arrow"


def load_and_combine_unprocessed(file_paths) -> pd.DataFrame:
    dfs = []
    for file_path in file_paths:
        print_info(f"  loading: {file_path}")
        df_table = read_arrow(file_path)
        print_info(f"  transforming..")
        dfs.append(transform_df_table(df_table))
        print_info(f"  done.\n")

    print_info(f"Loaded {len(dfs)} files!")
    print_info(f"  concatenating..")
    concated_df = pd.concat(dfs, axis=0)
    print_info(f"  done!")
    return concated_df


def process(settings):
    print_info(f"Determining individual file paths ({settings.path})")
    file_paths = get_file_paths(settings.path)

    print_info(f"Determining the 'run'")
    export_path = determine_run_from_file_paths(settings, file_paths)
    print_info(f"  combine files to a single arrow file: '{export_path}'")

    print_info(f"Identified {len(file_paths)} files to load!")
    processed_df = load_and_combine_unprocessed(file_paths)

    print_info(f"Writing processed DataFrame ({processed_df.shape[0]}x{processed_df.shape[1]}) to: {export_path}!")
    create_subdirs(export_path)
    write_arrow(processed_df, export_path)


def plot_df(settings, df: pd.DataFrame, axes=Optional[Axes], legend=True):
    func = DEFAULT_DIASHOW_PLOT_TYPE_CHOICES[settings.plot_type]

    ax: Optional[Axes] = None

    if axes is None:
        _, ax = plt.subplots()
    else:
        ax = axes

    func(ax, df)

    # ax.set_title('Scatter Plot of UL Bytes over Time')
    # ax.tick_params(axis='x', rotation=45)
    ax.set_xlabel('Timestamp (seconds)', fontsize=28) # TODO: Check timestamp unit
    ax.set_ylabel(settings.column, fontsize=28) # TODO: Convert column to proper name
    ax.tick_params(axis='x', labelsize=24)
    ax.tick_params(axis='y', labelsize=24)

    if legend:
        ax.legend(fontsize=18)

    if ax is None:
        plt.show()
    else:
        plt.draw()


def scatter_ajo(ax, df, column):
    delta_us = (df.index - df.index[0]).total_seconds() * 1e6
    ax.scatter(delta_us, df[column], label=column, s=50)


def plot(settings):
    print_info(f"Determining file path ({settings.path})")
    file_path = get_file_paths(settings.path)[0]
    print_info(f"  using: {file_path}")

    print_info(f"Reading file..")
    df = read_arrow(file_path)
    print_info(f" done!")

    df.index = (df.index - df.index[0]).astype('timedelta64[us]')
    _, ax = plt.subplots()
    scatter_ajo(ax, df, 'dl_prb')
    plt.show()
    _, ax = plt.subplots()
    plot_cdf(ax, df.dl_prb)
    plt.show()
    _, ax = plt.subplots()
    plot_cdf(ax, df.nof_rnti)
    plt.show()

    # df = plot_load_df(settings)
    # plot_df(settings, df, axes=None)


def diashow(settings):
    pass

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description='Display UL traffic patterns from a dataset.')
    parser.add_argument('--column',
                        type=str,
                        default=DEFAULT_COLUMN,
                        help=f'Column to visualize (default: {DEFAULT_COLUMN})')
    parser.add_argument('--reset-timestamps',
                        type=bool,
                        default=DEFAULT_RESET_TIMESTAMPS,
                        help=f'Reset timestamps to 00:00 (default: {DEFAULT_RESET_TIMESTAMPS})')
    parser.add_argument('--single-rnti',
                        type=str,
                        default=None,
                        help='Display only a single RNTI')
    parser.add_argument('--kbps',
                        type=bool,
                        default=DEFAULT_DIASHOW_KBPS,
                        help='Resample to kbps (default: {DEFAULT_DIASHOW_KBPS})')
    parser.add_argument('--plot-type',
                        type=str,
                        choices=list(DEFAULT_DIASHOW_PLOT_TYPE_CHOICES.keys()),
                        default=DEFAULT_DIASHOW_PLOT_TYPE,
                        help='The type of the plot (default: {DEFAULT_DIASHOW_PLOT_TYPE})')

    subparsers = parser.add_subparsers(dest='command', required=True)

    # diashow subcommand
    parser_diashow = subparsers.add_parser('diashow', help='Run diashow mode')

    # plot subcommand
    parser_standardize = subparsers.add_parser('plot', help='Show combined data')
    parser_standardize.add_argument('--path',
                                    type=str,
                                    default=DEFAULT_PLOT_PATH,
                                    help=f'Path to the dataset file (default: {DEFAULT_PLOT_PATH})')
    parser_standardize.add_argument('--combine',
                                    type=bool,
                                    default=DEFAULT_PLOT_COMBINE,
                                    help='Combine all .arrow files in --path (default: {DEFAULT_PLOT_COMBINE})')

    # process subcommand
    parser_process = subparsers.add_parser('process', help='Process raw DCI data')
    parser_process.add_argument('--path',
                                type=str,
                                default=DEFAULT_PROCESS_PATH,
                                help=f'Path to the dataset file (default: {DEFAULT_PROCESS_PATH})')
    parser_process.add_argument('--output-dir',
                                type=str,
                                default=DEFAULT_PROCESS_OUTPUT_DIR,
                                help='Output dir for processed .arrow file (default: {DEFAULT_PROCESS_OUTPUT_DIR})')


    args = parser.parse_args()

    if args.command == 'diashow':
        diashow(args)
    elif args.command == 'plot':
        plot(args)
    elif args.command == 'process':
        process(args)

