import React, { useState, useEffect } from 'react';
import { Button } from '@/new-components/Button';
import TextInput from '../../../Common/TextInput/TextInput';
import { InheritedRole } from '../../../../metadata/types';
import { focusYellowRing, inputStyles } from '../constants';

type Mode = 'create' | 'edit';

export type EditorProps = {
  allRoles: string[];

  //  Pass the Inherited Role object when editing an existing Role
  inheritedRole?: InheritedRole | null;

  //  Pass the the Role name while creating a new Object.
  inheritedRoleName?: string;

  onSave: (inheritedRole: InheritedRole) => void;

  isCollapsed: boolean;

  cancelCb: () => void;
};

const InheritedRolesEditor: React.FC<EditorProps> = ({
  allRoles,
  onSave,
  cancelCb,
  ...props
}) => {
  const [inheritedRoleName, setInheritedRoleName] = useState(
    props.inheritedRoleName
  );
  const [inheritedRole, setInheritedRole] = useState(props.inheritedRole);
  const [isCollapsed, setIsCollapsed] = useState(props.isCollapsed);

  type Option = {
    value: (typeof allRoles)[number];
    isChecked: true | false;
  };

  const [mode, setMode] = useState<Mode>(() =>
    inheritedRole ? 'edit' : 'create'
  );

  const defaultOptions = allRoles.map(role => ({
    value: role,
    isChecked:
      mode === 'create'
        ? false
        : inheritedRole?.role_set.includes(role) || false,
  }));

  const [options, setOptions] = useState(defaultOptions);

  useEffect(() => {
    setInheritedRoleName(props.inheritedRoleName);
    setInheritedRole(props.inheritedRole);
    setIsCollapsed(props.isCollapsed);
    const updatedMode = props.inheritedRole ? 'edit' : 'create';
    setMode(updatedMode);
    setOptions(
      allRoles.map(role => ({
        value: role,
        isChecked:
          updatedMode === 'create'
            ? false
            : props.inheritedRole?.role_set.includes(role) || false,
      }))
    );
  }, [
    props.inheritedRoleName,
    props.inheritedRole,
    props.isCollapsed,
    allRoles,
  ]);

  const [filterText, setFilterText] = useState('');

  const filterTextChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    e.persist();
    setFilterText(e.target.value);
  };

  const selectAll = () => {
    const allOptions = allRoles.map(role => ({ value: role, isChecked: true }));
    setOptions(allOptions);
  };

  const clearAll = () => {
    const allOptions = allRoles.map(role => ({
      value: role,
      isChecked: false,
    }));
    setOptions(allOptions);
  };

  const checkboxValueChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    e.persist();
    setOptions(
      options.map(option => {
        if (option.value !== e.target.value) return option;
        return { value: option.value, isChecked: !option.isChecked };
      })
    );
  };

  const saveRole = () => {
    const response: InheritedRole = {
      role_name: '',
      role_set: [],
    };

    if (mode === 'create') {
      response.role_name = inheritedRoleName || '';
    } else {
      response.role_name = inheritedRole?.role_name || '';
    }

    response.role_set = options
      .filter((option: Option) => option.isChecked)
      .map((option: Option) => option.value);

    onSave(response);
  };

  return (
    <>
      {!isCollapsed && (
        <div className="p-md bg-white border border-gray-200">
          <div>
            <div className="flex">
              <Button
                size="sm"
                onClick={() => {
                  cancelCb();
                }}
              >
                Cancel
              </Button>
              <div className="pl-md">
                {mode === 'create' ? (
                  <div>
                    <b>Create Role:</b> {inheritedRoleName}{' '}
                  </div>
                ) : (
                  <div>
                    <b>Edit Role:</b> {inheritedRole?.role_name}
                  </div>
                )}
              </div>
            </div>
            <hr className="my-md" />
            <div>
              <TextInput
                onChange={filterTextChange}
                value={filterText}
                placeholder="Filter Roles..."
                bsclass={`mb-xs ${inputStyles}`}
              />
              <div>
                <Button size="sm" onClick={selectAll}>
                  Select all
                </Button>{' '}
                <Button size="sm" onClick={clearAll}>
                  Clear all
                </Button>
              </div>
            </div>
            <br />
            <div>
              {!options.length
                ? 'No singular/Non-inherited Roles available'
                : options
                    .filter(
                      (option: Option) =>
                        option.value.includes(filterText) || !filterText.length
                    )
                    .map((option: Option, index) => (
                      <div key={index} className="flex items-center">
                        <input
                          className={`${focusYellowRing} !m-0 !mr-sm`}
                          type="checkbox"
                          checked={option.isChecked}
                          onChange={checkboxValueChange}
                          value={option.value}
                          required
                        />{' '}
                        <div>{option.value}</div>
                      </div>
                    ))}
            </div>
            <hr className="my-md" />
            <div>
              <Button
                mode="primary"
                onClick={saveRole}
                disabled={
                  !options.filter((option: Option) => option.isChecked).length
                }
              >
                Save Role
              </Button>
            </div>
          </div>
        </div>
      )}
    </>
  );
};

export default InheritedRolesEditor;
