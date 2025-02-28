import {
  OPERATION_ID_SYMBOL,
  OPERATION_NAME_SYMBOL,
  OPERATION_TYPE_SYMBOL,
  ROLE_SYMBOL,
  FILTER_TYPE_DROPDOWN,
  FILTER_TYPE_DROPDOWN_DEFAULT,
  FILTER_TYPE_INPUT,
  TIME_RANGE_SYMBOL,
  CLIENT_NAME_SYMBOL,
  TIME_RANGE_BY_HOUR,
  TIME_RANGE_BY_6_HOURS,
  TIME_RANGE_BY_12_HOURS,
  TIME_RANGE_BY_DAY,
  OPERATION_ID_DISPLAY,
  OPERATION_NAME_DISPLAY,
  OPERATION_TYPE_DISPLAY,
  ROLE_DISPLAY,
  TIME_RANGE_DISPLAY,
  CLIENT_NAME_DISPLAY,
  EMPTY_OPERATION_TYPE_SYMBOL,
  EMPTY_ROLE_SYMBOL,
  EMPTY_CLIENT_NAME_SYMBOL,
  SHOW_ONLY_ERRORS_SYMBOL,
  TRANSPORT_SYMBOL,
  TRANSPORT_DISPLAY,
} from '../constants';

export const actualTypeToAlias = {
  user_role: 'role',
};

export const defaultColumns = [
  'request_count',
  'average_response_size',
  'average_execution_time',
  'error_count',
];

export const headerTitleLabel = {
  average_execution_time: 'ms',
  average_response_size: 'kB',
};

export const aliasedNames = {
  role: 'user_role',
  success: 'is_error',
};

export const GROUP_BY_COLUMNS = [
  OPERATION_ID_SYMBOL,
  OPERATION_NAME_SYMBOL,
  OPERATION_TYPE_SYMBOL,
  ROLE_SYMBOL,
  CLIENT_NAME_SYMBOL,
  TRANSPORT_SYMBOL,
];

export const DEFAULT_GROUP_BY = [OPERATION_NAME_SYMBOL];

export const FILTER_MAP = {
  [TIME_RANGE_SYMBOL]: FILTER_TYPE_DROPDOWN_DEFAULT,
  [OPERATION_ID_SYMBOL]: FILTER_TYPE_INPUT,
  [OPERATION_NAME_SYMBOL]: FILTER_TYPE_INPUT,
  [OPERATION_TYPE_SYMBOL]: FILTER_TYPE_DROPDOWN,
  [ROLE_SYMBOL]: FILTER_TYPE_DROPDOWN,
  [CLIENT_NAME_SYMBOL]: FILTER_TYPE_DROPDOWN,
  [TRANSPORT_SYMBOL]: FILTER_TYPE_DROPDOWN,
};

export const TITLE_MAP = {
  [OPERATION_ID_SYMBOL]: OPERATION_ID_DISPLAY,
  [OPERATION_NAME_SYMBOL]: OPERATION_NAME_DISPLAY,
  [OPERATION_TYPE_SYMBOL]: OPERATION_TYPE_DISPLAY,
  [ROLE_SYMBOL]: ROLE_DISPLAY,
  [TIME_RANGE_SYMBOL]: TIME_RANGE_DISPLAY,
  [CLIENT_NAME_SYMBOL]: CLIENT_NAME_DISPLAY,
  [TRANSPORT_SYMBOL]: TRANSPORT_DISPLAY,
};

export const NO_TITLE_MAP = {
  [OPERATION_TYPE_SYMBOL]: EMPTY_OPERATION_TYPE_SYMBOL,
  [ROLE_SYMBOL]: EMPTY_ROLE_SYMBOL,
  [CLIENT_NAME_SYMBOL]: EMPTY_CLIENT_NAME_SYMBOL,
};

export const timeRangeFilters = [
  TIME_RANGE_BY_HOUR,
  TIME_RANGE_BY_6_HOURS,
  TIME_RANGE_BY_12_HOURS,
  TIME_RANGE_BY_DAY,
];

export const toggleFilters = [SHOW_ONLY_ERRORS_SYMBOL];
export const singleSelectFilters = [TIME_RANGE_SYMBOL];
